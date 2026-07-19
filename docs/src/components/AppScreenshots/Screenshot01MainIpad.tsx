import React from 'react';
import ScreenshotComposition from './ScreenshotComposition';

export default function Screenshot01MainIpad() {
  return (
    <ScreenshotComposition
      background="#f3efe2"
      textColor="#1a1a1a"
      headline="Manage your photos."
      screenshotSrc="https://public.getlasco.app/screen_main_ipad.png"
      screenshotAlt="Lasco library screen showing the main photo grid on iPad"
      shotAspectRatio="1032 / 1376"
      screenshotCropTopPercent={3}
      rotate={0}
      shiftX={0}
      shiftY={70}
    />
  );
}
