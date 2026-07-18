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
  const {toPng} = await import('html-to-image');
  // Force the canvas to the exact target pixel dimensions instead of deriving
  // them from a single pixelRatio scalar, since the frame's aspect ratio
  // (430:932) doesn't exactly match the App Store target's.
  const dataUrl = await toPng(node, {pixelRatio: 1, canvasWidth: width, canvasHeight: height});
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
