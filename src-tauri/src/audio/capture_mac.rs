//! macOS: Core Audio 프로세스 탭 캡처 (macOS 14.2+). 시스템 전체, 또는 고른 앱들의 소리를 합친 것.
//!
//! 탭(CATapDescription)은 고른 프로세스들(또는 이 앱을 뺀 전부)의 출력을 스테레오로 섞어 준다.
//! 탭은 그 자체로 읽을 수 없어서, 기본 출력 장치를 시계로 쓰는 비공개 aggregate 장치에 붙이고
//! 그 장치의 IOProc에서 입력 버퍼를 읽는다 (Apple 예제 AudioCap과 같은 구성).
//! 처음 열 때 macOS가 "시스템 오디오 녹음" 권한을 묻는다 (Info.plist의 NSAudioCaptureUsageDescription).

use super::{sessions, Msg};
use crate::config::Source;
use crate::state::{lock, Shared};
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::AnyThread;
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceIsStackedKey, kAudioAggregateDeviceMainSubDeviceKey,
    kAudioAggregateDeviceNameKey, kAudioAggregateDeviceSubDeviceListKey, kAudioAggregateDeviceTapAutoStartKey,
    kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceUID,
    kAudioDevicePropertyNominalSampleRate, kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectSystemObject,
    kAudioSubDeviceUIDKey, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey, kAudioTapPropertyFormat,
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart, AudioDeviceStop,
    AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap, AudioHardwareDestroyAggregateDevice,
    AudioHardwareDestroyProcessTap, AudioObjectID, CATapDescription, CATapMuteBehavior,
};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::{
    kCFAllocatorDefault, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFArray,
    CFDictionary, CFMutableDictionary, CFRetained, CFString,
};
use objc2_foundation::{NSArray, NSNumber, NSString};
use std::ffi::{c_void, CStr};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};

/// 이 시간 동안 버퍼가 오지 않으면 0을 채워 분석 시계를 계속 돌린다.
const SILENCE_AFTER: Duration = Duration::from_millis(60);

type OSStatus = i32;

fn check(status: OSStatus, what: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{what} failed (Core Audio error {status})"))
    }
}

// ---- CoreFoundation 사전 만들기 (aggregate 장치 설명) ----

fn cf_key(key: &CStr) -> CFRetained<CFString> {
    unsafe { CFString::with_c_string(kCFAllocatorDefault, key.as_ptr(), 0x0800_0100 /* UTF-8 */) }.unwrap()
}

/// (키, 값 포인터) 쌍으로 사전을 만든다. 값은 CF/NS 객체여야 한다 (사전이 retain한다).
fn cf_dict(pairs: &[(&CStr, *const c_void)]) -> CFRetained<CFMutableDictionary> {
    unsafe {
        let dict =
            CFMutableDictionary::new(kCFAllocatorDefault, pairs.len() as _, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks)
                .unwrap();
        for (key, value) in pairs {
            let key = cf_key(key);
            CFMutableDictionary::set_value(Some(dict.as_ref()), &*key as *const CFString as *const c_void, *value);
        }
        dict
    }
}

fn cf_array(items: &[*const c_void]) -> CFRetained<CFArray> {
    unsafe { CFArray::new(kCFAllocatorDefault, items.as_ptr() as *mut *const c_void, items.len() as _, &kCFTypeArrayCallBacks) }.unwrap()
}

fn ptr<T>(r: &T) -> *const c_void {
    r as *const T as *const c_void
}

// ---- 장치 ----

/// 기본 출력 장치 (id, UID).
fn default_output() -> Result<(AudioObjectID, String), String> {
    let id: AudioObjectID = sessions::get_prop(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyDefaultOutputDevice)
        .filter(|&id| id != 0)
        .ok_or("There's no sound output device.")?;
    let raw: usize = sessions::get_prop(id, kAudioDevicePropertyDeviceUID).unwrap_or(0);
    // 이 속성은 소유권을 넘겨준다 (+1 retain).
    let uid = NonNull::new(raw as *mut CFString)
        .map(|p| unsafe { CFRetained::from_raw(p) })
        .ok_or("Couldn't read the output device.")?;
    Ok((id, uid.to_string()))
}

fn default_output_id() -> Option<AudioObjectID> {
    sessions::get_prop(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyDefaultOutputDevice)
}

// ---- IOProc ----

/// IOProc 콜백이 쓰는 상태. `Tap`이 소유하고, IO를 멈춘 뒤에 해제한다.
struct Ctx {
    tx: SyncSender<Msg>,
    started: Instant,
    /// 마지막으로 버퍼가 온 시각 (`started`로부터 ms).
    last_ms: AtomicU64,
    /// 분석 스레드가 사라졌다.
    dead: AtomicBool,
}

unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> OSStatus {
    let Some(ctx) = (client as *const Ctx).as_ref() else { return 0 };
    let list = input.as_ptr();
    let count = (*list).mNumberBuffers as usize;
    let buffers = std::ptr::addr_of!((*list).mBuffers) as *const AudioBuffer;

    // 버퍼 하나에 여러 채널(인터리브)이든, 채널마다 버퍼(논인터리브)든 모두 mono로 섞는다. 탭은 항상 float32다.
    let mut channels = 0usize;
    let mut frames = usize::MAX;
    for i in 0..count {
        let b = &*buffers.add(i);
        if b.mData.is_null() {
            continue;
        }
        let ch = (b.mNumberChannels as usize).max(1);
        channels += ch;
        frames = frames.min(b.mDataByteSize as usize / (4 * ch));
    }
    if channels == 0 || frames == 0 || frames == usize::MAX {
        return 0;
    }
    let mut mono = vec![0.0f32; frames];
    for i in 0..count {
        let b = &*buffers.add(i);
        if b.mData.is_null() {
            continue;
        }
        let ch = (b.mNumberChannels as usize).max(1);
        let data = std::slice::from_raw_parts(b.mData as *const f32, frames * ch);
        for (o, frame) in mono.iter_mut().zip(data.chunks_exact(ch)) {
            *o += frame.iter().sum::<f32>();
        }
    }
    let scale = 1.0 / channels as f32;
    mono.iter_mut().for_each(|v| *v *= scale);

    ctx.last_ms.store(ctx.started.elapsed().as_millis() as u64, Ordering::Release);
    // 분석이 밀리면 버린다 (지연을 쌓지 않는다).
    if let Err(TrySendError::Disconnected(_)) = ctx.tx.try_send(Msg::Samples(mono)) {
        ctx.dead.store(true, Ordering::Release);
    }
    0
}

// ---- 탭 + aggregate 장치 ----

struct Tap {
    tap: AudioObjectID,
    aggregate: AudioObjectID,
    io: AudioDeviceIOProcID,
    ctx: *mut Ctx,
    rate: u32,
}

impl Tap {
    /// `include`가 있으면 그 프로세스 객체들만, 없으면 이 앱을 뺀 시스템 전체를 탭한다.
    fn new(include: Option<&[AudioObjectID]>, output_uid: &str) -> Result<Tap, String> {
        let mut t = Tap { tap: 0, aggregate: 0, io: None, ctx: std::ptr::null_mut(), rate: 48000 };
        unsafe {
            let desc: Retained<CATapDescription> = match include {
                Some(objects) => {
                    let nums: Vec<Retained<NSNumber>> = objects.iter().map(|&o| NSNumber::new_u32(o)).collect();
                    CATapDescription::initStereoMixdownOfProcesses(CATapDescription::alloc(), &NSArray::from_retained_slice(&nums))
                }
                None => {
                    let me = std::process::id() as i32;
                    let nums: Vec<Retained<NSNumber>> =
                        sessions::processes().into_iter().filter(|p| p.pid == me).map(|p| NSNumber::new_u32(p.object)).collect();
                    CATapDescription::initStereoGlobalTapButExcludeProcesses(CATapDescription::alloc(), &NSArray::from_retained_slice(&nums))
                }
            };
            desc.setName(&NSString::from_str("PlushBounce"));
            desc.setPrivate(true);
            // 들은 소리는 그대로 스피커로도 나간다.
            desc.setMuteBehavior(CATapMuteBehavior::Unmuted);

            check(AudioHardwareCreateProcessTap(Some(&desc), &mut t.tap), "Creating the audio tap")?;

            let tap_uid = desc.UUID().UUIDString();
            let pid = std::process::id();
            let agg_uid = CFString::from_str(&format!("com.plushbounce.tap.{pid}.{}", t.tap));
            let agg_name = CFString::from_str("PlushBounce Tap");
            let out_uid = CFString::from_str(output_uid);
            let yes = NSNumber::new_bool(true);
            let no = NSNumber::new_bool(false);

            let sub_device = cf_dict(&[(kAudioSubDeviceUIDKey, ptr(&*out_uid))]);
            let sub_tap = cf_dict(&[(kAudioSubTapUIDKey, ptr(&*tap_uid)), (kAudioSubTapDriftCompensationKey, ptr(&*yes))]);
            let sub_devices = cf_array(&[ptr(&*sub_device)]);
            let taps = cf_array(&[ptr(&*sub_tap)]);
            let description = cf_dict(&[
                (kAudioAggregateDeviceNameKey, ptr(&*agg_name)),
                (kAudioAggregateDeviceUIDKey, ptr(&*agg_uid)),
                (kAudioAggregateDeviceMainSubDeviceKey, ptr(&*out_uid)),
                (kAudioAggregateDeviceIsPrivateKey, ptr(&*yes)),
                (kAudioAggregateDeviceIsStackedKey, ptr(&*no)),
                (kAudioAggregateDeviceTapAutoStartKey, ptr(&*yes)),
                (kAudioAggregateDeviceSubDeviceListKey, ptr(&*sub_devices)),
                (kAudioAggregateDeviceTapListKey, ptr(&*taps)),
            ]);
            let description: &CFDictionary = &description;
            check(AudioHardwareCreateAggregateDevice(description, NonNull::from(&mut t.aggregate)), "Creating the capture device")?;
        }

        // 탭이 알려 주는 형식의 샘플레이트로 받는다. 못 읽으면 장치 샘플레이트.
        let format: Option<AudioStreamBasicDescription> = sessions::get_prop(t.tap, kAudioTapPropertyFormat);
        let rate = format
            .map(|f| f.mSampleRate)
            .filter(|r| *r > 1000.0)
            .or_else(|| sessions::get_prop::<f64>(t.aggregate, kAudioDevicePropertyNominalSampleRate).filter(|r| *r > 1000.0))
            .unwrap_or(48000.0);
        t.rate = rate.round() as u32;
        Ok(t)
    }

    fn start(&mut self, tx: SyncSender<Msg>) -> Result<(), String> {
        let ctx = Box::into_raw(Box::new(Ctx { tx, started: Instant::now(), last_ms: AtomicU64::new(0), dead: AtomicBool::new(false) }));
        self.ctx = ctx;
        unsafe {
            check(
                AudioDeviceCreateIOProcID(self.aggregate, Some(io_proc), ctx as *mut c_void, NonNull::from(&mut self.io)),
                "Opening the capture device",
            )?;
            check(AudioDeviceStart(self.aggregate, self.io), "Starting capture")?;
        }
        Ok(())
    }

    fn ctx(&self) -> &Ctx {
        // start() 뒤에만 부른다.
        unsafe { &*self.ctx }
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        unsafe {
            if self.io.is_some() {
                let _ = AudioDeviceStop(self.aggregate, self.io);
                let _ = AudioDeviceDestroyIOProcID(self.aggregate, self.io);
            }
            if self.aggregate != 0 {
                let _ = AudioHardwareDestroyAggregateDevice(self.aggregate);
            }
            if self.tap != 0 {
                let _ = AudioHardwareDestroyProcessTap(self.tap);
            }
            // IO가 멈춘 뒤라 콜백이 더는 ctx를 보지 않는다.
            if !self.ctx.is_null() {
                drop(Box::from_raw(self.ctx));
            }
        }
    }
}

/// 캡처를 시작해 스트림이 끝나거나 다시 연결해야 할 때까지 돈다.
/// `Ok`는 소스/장치가 바뀌어 다시 시작해야 함을, `Err`는 실패를 뜻한다.
pub fn run(state: &Shared, tx: &SyncSender<Msg>, on_running: &dyn Fn()) -> Result<(), String> {
    if AnyClass::get(c"CATapDescription").is_none() {
        return Err("Capturing sound needs macOS 14.2 or later.".into());
    }
    let version = state.audio_version.load(Ordering::Acquire);
    let source = lock(&state.config).audio.source.clone();
    let wanted: Vec<String> = match &source {
        Source::Apps { names } => names.clone(),
        Source::Process { name, .. } => vec![name.clone()],
        Source::System => Vec::new(),
    };

    let mut objects: Vec<AudioObjectID> = Vec::new();
    if !wanted.is_empty() {
        objects = sessions::objects_for(&wanted);
        if objects.is_empty() {
            let list = wanted.join(", ");
            return Err(format!("The selected apps ({list}) are not running. Start one or choose another source."));
        }
    }

    let (device_id, output_uid) = default_output()?;
    let mut tap = Tap::new((!wanted.is_empty()).then_some(objects.as_slice()), &output_uid)?;
    let rate = tap.rate;
    tx.send(Msg::Format(rate)).map_err(|_| "The analysis thread stopped.".to_string())?;
    tap.start(tx.clone())?;
    on_running();

    let mut last_fill = Instant::now();
    let mut last_check = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(20));
        let now = Instant::now();
        let ctx = tap.ctx();
        if ctx.dead.load(Ordering::Acquire) {
            return Err("The analysis thread stopped.".into());
        }

        // 장치가 버퍼를 주지 않는 동안(잠자기 등)에도 분석 시계가 돌도록 0을 채운다.
        let last_data = ctx.started + Duration::from_millis(ctx.last_ms.load(Ordering::Acquire));
        let quiet_since = last_data.max(last_fill);
        if now.duration_since(quiet_since) >= SILENCE_AFTER {
            let gap = now.duration_since(quiet_since).as_secs_f32().min(0.2);
            let k = (gap * rate as f32) as usize;
            if k > 0 {
                match tx.try_send(Msg::Samples(vec![0.0; k])) {
                    Err(TrySendError::Disconnected(_)) => return Err("The analysis thread stopped.".into()),
                    _ => last_fill = now,
                }
            }
        }

        if state.audio_version.load(Ordering::Acquire) != version && lock(&state.config).audio.source != source {
            break;
        }
        if now.duration_since(last_check) >= Duration::from_secs(1) {
            last_check = now;
            // 기본 출력 장치가 바뀌면 새 장치를 시계로 다시 만든다.
            if default_output_id() != Some(device_id) {
                break;
            }
            if !wanted.is_empty() && sessions::objects_for(&wanted) != objects {
                // 고른 앱이 꺼졌거나 켜졌거나, 소리를 내는 도우미 프로세스가 새로 생겼다.
                break;
            }
        }
    }
    Ok(())
}
