import React, {useRef} from 'react';
import Layout from '@theme/Layout';
import PhoneFrame from '@site/src/components/AppScreenshots/PhoneFrame';
import ScreenshotDownloadButtons from '@site/src/components/AppScreenshots/ScreenshotDownloadButtons';
import Screenshot01Main from '@site/src/components/AppScreenshots/Screenshot01Main';
import Screenshot02Sync from '@site/src/components/AppScreenshots/Screenshot02Sync';
import Screenshot03Private from '@site/src/components/AppScreenshots/Screenshot03Private';
import styles from './app-screenshots.module.css';

const SCREENSHOTS = [
  {number: '01', slug: 'main', Content: Screenshot01Main},
  {number: '02', slug: 'sync', Content: Screenshot02Sync},
  {number: '03', slug: 'private', Content: Screenshot03Private},
];

const DEVICE_VARIANTS = [
  {id: '6.5in', label: '6.5" · 1242×2688', width: 1242, height: 2688},
];

function ScreenshotRow({
  filenameBase,
  Content,
  variant,
}: {
  filenameBase: string;
  Content: React.ComponentType;
  variant: (typeof DEVICE_VARIANTS)[number];
}) {
  const frameRef = useRef<HTMLDivElement>(null);
  return (
    <div className={styles.row}>
      <span className={styles.rowLabel}>{variant.label}</span>
      <PhoneFrame ref={frameRef}>
        <Content />
      </PhoneFrame>
      <ScreenshotDownloadButtons
        targetRef={frameRef}
        width={variant.width}
        height={variant.height}
        filename={`${filenameBase}-${variant.id}-${variant.width}x${variant.height}.png`}
        label={`${variant.width}×${variant.height}`}
      />
    </div>
  );
}

function ScreenshotCard({
  number,
  slug,
  Content,
}: {
  number: string;
  slug: string;
  Content: React.ComponentType;
}) {
  const filenameBase = `${number}-${slug}`;
  return (
    <div className={styles.card}>
      <span className={styles.cardLabel}>{number} · {slug}</span>
      <div className={styles.rowGroup}>
        {DEVICE_VARIANTS.map(variant => (
          <ScreenshotRow
            key={variant.id}
            filenameBase={filenameBase}
            Content={Content}
            variant={variant}
          />
        ))}
      </div>
    </div>
  );
}

export default function AppScreenshotsPage() {
  return (
    <Layout title="App Store Screenshots">
      <main className={styles.page}>
        <h1 className={styles.title}>App Store Screenshots</h1>
        <p className={styles.intro}>
          Each card below is a live composition of a real app screenshot plus marketing copy.
          Every card shows both target device dimensions, and each download button rasterizes
          that row to its exact App Store resolution.
        </p>
        <div className={styles.grid}>
          {SCREENSHOTS.map(({number, slug, Content}) => (
            <ScreenshotCard key={slug} number={number} slug={slug} Content={Content} />
          ))}
        </div>
      </main>
    </Layout>
  );
}
