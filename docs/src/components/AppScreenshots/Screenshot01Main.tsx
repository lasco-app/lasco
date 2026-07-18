import React from 'react';
import ScreenshotComposition from './ScreenshotComposition';

export default function Screenshot01Main() {
  return (
    <ScreenshotComposition
      background="#f3efe2"
      textColor="#1a1a1a"
      headline="Manage your photos."
      screenshotSrc="https://public.getlasco.app/screen_main.webp"
      screenshotAlt="Lasco library screen showing the main photo grid"
      rotate={0}
      shiftX={0}
      shiftY={70}
    />
  );
}
