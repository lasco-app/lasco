import React from 'react';
import ScreenshotComposition from './ScreenshotComposition';

export default function Screenshot03PrivateIpad() {
  return (
    <ScreenshotComposition
      background="#f3efe2"
      textColor="#1a1a1a"
      headline="Encrypted before it leaves your phone."
      screenshotSrc="https://public.getlasco.app/screen_album_ipad.png"
      screenshotAlt="Lasco private album screen on iPad"
      screenshotObjectPosition="bottom center"
      shotAspectRatio="1032 / 1376"
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
