import type {ReactNode} from 'react';
import Layout from '@theme/Layout';

import styles from './roadmap.module.css';

const ROADMAP_ITEMS: string[] = [
  'Android app',
  'Mac app',
  'USB storage support',
  'NAS storage support',
  'Lasco Cloud (subscription to get two remote storages in two locations)',
  'ML photo analysis',
];

export default function Roadmap(): ReactNode {
  return (
    <Layout title="Roadmap" description="What is coming next for Lasco.">
      <main className={styles.wrapper}>
        <h1 className={styles.title}>Roadmap</h1>
        <p className={styles.subtitle}>Within a few weeks</p>
        <div className={styles.grid}>
          {ROADMAP_ITEMS.map((item) => (
            <div key={item} className={styles.card}>
              {item}
            </div>
          ))}
        </div>
      </main>
    </Layout>
  );
}
