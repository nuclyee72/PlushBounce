<script lang="ts">
  const paths: Record<string, string> = {
    plus: "M12 5v14M5 12h14",
    x: "M6 6l12 12M18 6L6 18",
    check: "M5 12.5l4.5 4.5L19 7.5",
    minus: "M5 12h14",
    clock: "M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17ZM12 7.5V12l3 2",
    spread: "M3 12h18M6.5 8.5 3 12l3.5 3.5M17.5 8.5 21 12l-3.5 3.5M12 7v10",
    trash: "M4 7h16M9 7V4.5h6V7M6.5 7l1 13h9l1-13M10 11v5.5M14 11v5.5",
    circle: "M12 4a8 8 0 1 0 0 16 8 8 0 0 0 0-16Z",
    speaker: "M4 9.5h3.5L12 5v14l-4.5-4.5H4v-5ZM16 9a4 4 0 0 1 0 6M18.5 6.5a7.5 7.5 0 0 1 0 11",
    refresh: "M20 11a8 8 0 1 0-2.3 5.7M20 4v7h-7",
    eye: "M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12Zm10 3a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z",
    eyeOff: "M3 3l18 18M10.6 5.1A10.9 10.9 0 0 1 12 5c6.4 0 10 7 10 7a17 17 0 0 1-3.2 4M6.5 6.6A16.600 16.600 0 0 0 2 12s3.6 7 10 7c1.600 0 3-.4 4.300-1",
    flip: "M12 3v18M8 7l-5 5 5 5V7Zm8 0l5 5-5 5V7Z",
    onset: "M3 17h3l2-11 3 14 3-9 2 6h5",
    tempo: "M8 21h8l-2.500-17h-3L8 21Zm4-5l6-9M7 17h10",
    pet: "M3 5h18v11H3zM8 20h8M12 16v4M9 12a3 3 0 0 1 6 0",
    wallpaper: "M3 5h18v14H3zM3 15l5-5 4 4 3-3 6 6M15.500 9.500h.01",
    edit: "M4 20h4L19 9l-4-4L4 16v4ZM13 7l4 4",
    download: "M12 4v11M7 11l5 5 5-5M5 20h14",
    upload: "M12 16V5M7 9l5-5 5 5M5 20h14",
    image: "M4 5h16v14H4zM4 15l4-4 4 4 3-3 5 5M15 9.500h.01",
    auto: "M4 12a8 8 0 0 1 14-5.300M20 4v5h-5M20 12a8 8 0 0 1-14 5.300M4 20v-5h5",
    power: "M12 3v9M6.300 7.300a8 8 0 1 0 11.400 0",
  };
  let { name }: { name: string } = $props();
</script>

<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  <path d={paths[name]} />
</svg>
