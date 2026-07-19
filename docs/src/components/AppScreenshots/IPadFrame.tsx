import React, {type ReactNode} from 'react';
import styles from './AppScreenshots.module.css';

// Authored at half the 13" iPad Pro App Store target's exact pixel
// dimensions (2064x2752), then rasterized straight to that resolution
// on download.
export const FRAME_WIDTH = 1032;
export const FRAME_HEIGHT = 1376;

type Props = {
  children: ReactNode;
};

const IPadFrame = React.forwardRef<HTMLDivElement, Props>(function IPadFrame(
  {children},
  ref,
) {
  return (
    <div ref={ref} className={styles.ipadFrame}>
      {children}
    </div>
  );
});

export default IPadFrame;
