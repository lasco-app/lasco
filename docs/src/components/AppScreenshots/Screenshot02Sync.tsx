import React from 'react';
import ScreenshotComposition from './ScreenshotComposition';

export default function Screenshot02Sync() {
  return (
    <ScreenshotComposition
      background="#f3efe2"
      textColor="#1a1a1a"
      headline="Sync your photos to S3 servers."
      screenshotSrc="https://public.getlasco.app/screen_remote.webp"
      screenshotAlt="Lasco library screen showing S3 sync"
      rotate={-8}
      shiftX={38}
      shiftY={70}
    />
  );
}
