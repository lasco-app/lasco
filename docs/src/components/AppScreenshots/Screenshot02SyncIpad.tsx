import React from 'react';
import ScreenshotComposition from './ScreenshotComposition';

export default function Screenshot02SyncIpad() {
  return (
    <ScreenshotComposition
      background="#f3efe2"
      textColor="#1a1a1a"
      headline="Sync your photos to S3 servers."
      screenshotSrc="https://public.getlasco.app/screen_remote_ipadfixs3.png"
      screenshotAlt="Lasco library screen showing S3 sync on iPad"
      shotAspectRatio="1032 / 1376"
      screenshotCropTopPercent={3}
      rotate={-8}
      shiftX={38}
      shiftY={70}
    />
  );
}
