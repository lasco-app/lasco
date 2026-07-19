import React, {type CSSProperties} from 'react';
import styles from './AppScreenshots.module.css';

type Props = {
  background: string;
  textColor: string;
  headline: string;
  screenshotSrc: string;
  screenshotAlt: string;
  screenshotObjectPosition?: string;
  rotate: number;
  shiftX: number;
  shiftY?: number;
  mascotSrc?: string;
  mascotStyle?: CSSProperties;
  shotAspectRatio?: string;
  screenshotCropTopPercent?: number;
};

export default function ScreenshotComposition({
  background,
  textColor,
  headline,
  screenshotSrc,
  screenshotAlt,
  screenshotObjectPosition,
  rotate,
  shiftX,
  shiftY = 0,
  mascotSrc,
  mascotStyle,
  shotAspectRatio,
  screenshotCropTopPercent = 0,
}: Props) {
  return (
    <div className={styles.stage} style={{background}}>
      <div className={styles.headlineBlock}>
        <p className={styles.headline} style={{color: textColor}}>
          {headline}
        </p>
      </div>
      {mascotSrc && (
        <img
          src={mascotSrc}
          aria-hidden="true"
          className={styles.mascot}
          style={mascotStyle}
        />
      )}
      <div
        className={styles.shotWrap}
        style={{
          bottom: `calc(20px + ${shiftY}px)`,
          transform: `translateX(calc(-50% + ${shiftX}px)) rotate(${rotate}deg)`,
          aspectRatio: shotAspectRatio,
        }}
      >
        <img
          src={screenshotSrc}
          alt={screenshotAlt}
          className={styles.shotImg}
          style={{
            objectPosition: screenshotObjectPosition,
            ...(screenshotCropTopPercent > 0
              ? {
                  transformOrigin: 'top',
                  transform: `translateY(-${screenshotCropTopPercent}%) scale(${
                    1 + screenshotCropTopPercent / 100
                  })`,
                }
              : {}),
          }}
        />
      </div>
    </div>
  );
}
