import React from 'react';
import ScreenshotComposition from './ScreenshotComposition';

export default function Screenshot03Private() {
  return (
    <ScreenshotComposition
      background="#f3efe2"
      textColor="#1a1a1a"
      headline="Encrypted before it leaves your phone."
      screenshotSrc="https://public.getlasco.app/screen_album2.webp"
      screenshotAlt="Lasco private album screen"
      screenshotObjectPosition="bottom center"
      rotate={6}
      shiftX={0}
      mascotSrc="https://public.getlasco.app/mascot_encrypted_0_5x.png"
      mascotStyle={{
        width: 345,
        right: -30,
        bottom: -110,
      }}
    />
  );
}
