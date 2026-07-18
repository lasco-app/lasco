import React, {type ReactNode} from 'react';
import styles from './AppScreenshots.module.css';

// Authored at iPhone 6.7" point size (430x932pt), then rasterized straight to
// the 6.5" App Store target's exact pixel dimensions (1242x2688) on download.
export const FRAME_WIDTH = 430;
export const FRAME_HEIGHT = 932;

type Props = {
  children: ReactNode;
};

const PhoneFrame = React.forwardRef<HTMLDivElement, Props>(function PhoneFrame(
  {children},
  ref,
) {
  return (
    <div ref={ref} className={styles.frame}>
      {children}
    </div>
  );
});

export default PhoneFrame;
