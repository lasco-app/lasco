import type {ReactNode} from 'react';
import Layout from '@theme/Layout';

import styles from './roadmap.module.css';

type RoadmapItem = {
  title: string;
  detail?: string;
  state?: 'done' | 'coming';
};

type RoadmapMonth = {
  month: string;
  complete?: boolean;
  items: RoadmapItem[];
};

const ROADMAP: RoadmapMonth[] = [
  {
    month: 'August 2026',
    complete: true,
    items: [
      {title: 'Google Play release', state: 'done'},
      {title: 'Lasco Cloud', detail: '50 GB of storage available', state: 'done'},
    ],
  },
  {
    month: 'September 2026',
    items: [
      {title: 'App Store release', state: 'coming'},
      {title: 'USB storage support'},
      {title: 'NAS support'},
    ],
  },
  {
    month: 'October 2026',
    items: [
      {title: 'Desktop importer app', state: 'coming'},
      {title: 'ML photo analysis', state: 'coming'},
    ],
  },
];

export default function Roadmap(): ReactNode {
  return (
    <Layout title="Roadmap" description="What is live and what is coming next for Lasco.">
      <main className={styles.wrapper}>
        <header className={styles.intro}>
          <h1 className={styles.title}>Roadmap</h1>
        </header>

        <ol className={styles.timeline}>
          {ROADMAP.map(({month, complete, items}) => (
            <li key={month} className={styles.month}>
              <div className={`${styles.marker} ${complete ? styles.complete : ''}`} aria-hidden="true" />
              <div className={styles.monthHeader}>
                <h2>{month}</h2>
              </div>
              <ul className={styles.items}>
                {items.map(({title, detail, state}) => (
                  <li key={title} className={`${styles.item} ${state === 'done' ? styles.done : ''}`}>
                    <span className={styles.itemMark} aria-hidden="true">
                      {state === 'done' ? '✓' : '+'}
                    </span>
                    <div>
                      <div className={styles.itemHeader}>
                        <h3>{title}</h3>
                        {state === 'coming' && <span className={styles.coming}>Coming</span>}
                      </div>
                      {detail && <p>{detail}</p>}
                    </div>
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ol>
      </main>
    </Layout>
  );
}
