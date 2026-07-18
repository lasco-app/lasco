import React, {type ReactNode} from 'react';
import styles from './AppScreenshots.module.css';

// Authored once at iPhone 6.7" point size (430x932pt). Exported at pixelRatio 3
// for the 6.7" target (1290x2796px) and at pixelRatio 1242/430 for the 6.5"
// target (1242x2688px) -- the two device aspect ratios differ by under 0.2%.
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
