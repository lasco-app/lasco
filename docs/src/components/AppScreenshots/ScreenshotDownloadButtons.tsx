import React, {type RefObject} from 'react';
import styles from './AppScreenshots.module.css';

type Props = {
  targetRef: RefObject<HTMLElement | null>;
  width: number;
  height: number;
  filename: string;
  label: string;
};

async function downloadAt(node: HTMLElement, width: number, height: number, filename: string) {
  const {toCanvas} = await import('html-to-image');
  // Force the canvas to the exact target pixel dimensions instead of deriving
  // them from a single pixelRatio scalar, since the frame's aspect ratio
  // (430:932) doesn't exactly match the App Store target's.
  const sourceCanvas = await toCanvas(node, {pixelRatio: 1, canvasWidth: width, canvasHeight: height});
  // Redraw onto an opaque canvas so the exported PNG has no alpha channel,
  // since App Store Connect rejects screenshots that carry one.
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const ctx = canvas.getContext('2d', {alpha: false});
  if (!ctx) {
    throw new Error('2D canvas context unavailable');
  }
  ctx.drawImage(sourceCanvas, 0, 0);
  const dataUrl = canvas.toDataURL('image/png');
  const link = document.createElement('a');
  link.download = filename;
  link.href = dataUrl;
  link.click();
}

export default function ScreenshotDownloadButtons({targetRef, width, height, filename, label}: Props) {
  return (
    <div className={styles.downloadRow}>
      <button
        type="button"
        className={styles.downloadButton}
        onClick={() => {
          if (targetRef.current) {
            void downloadAt(targetRef.current, width, height, filename);
          }
        }}
      >
        Download {label}
      </button>
    </div>
  );
}
