import React, {type RefObject} from 'react';
import styles from './AppScreenshots.module.css';

type Props = {
  targetRef: RefObject<HTMLElement | null>;
  pixelRatio: number;
  filename: string;
  label: string;
};

async function downloadAt(node: HTMLElement, pixelRatio: number, filename: string) {
  const {toPng} = await import('html-to-image');
  const dataUrl = await toPng(node, {pixelRatio});
  const link = document.createElement('a');
  link.download = filename;
  link.href = dataUrl;
  link.click();
}

export default function ScreenshotDownloadButtons({targetRef, pixelRatio, filename, label}: Props) {
  return (
    <div className={styles.downloadRow}>
      <button
        type="button"
        className={styles.downloadButton}
        onClick={() => {
          if (targetRef.current) {
            void downloadAt(targetRef.current, pixelRatio, filename);
          }
        }}
      >
        Download {label}
      </button>
    </div>
  );
}
