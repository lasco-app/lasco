import React, {type ReactNode} from 'react';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';

import styles from './index.module.css';

// =============================================
// Screenshot Scroll Section
// =============================================

const SCREENSHOT_PANELS = [
  {
    id: 'intro',
    img: 'https://public.getlasco.app/screen_main.webp',
    title: '',
    desc: 'Lasco is a client side app that syncs your photos to (soon) any kind of file storage.',
  },
  {
    id: 'timeline',
    img: 'https://public.getlasco.app/screen_remote.webp',
    title: 'Sync your photos to S3',
    desc: 'Push your library to S3 (and soon Lasco Cloud, a NAS, or a local hard drive). Everything is stored as regular (encrypted) files!',
  },
  {
    id: 'albums',
    img: 'https://public.getlasco.app/screen_album2.webp',
    title: 'Organize your photos',
    desc: 'Organize your photos into albums that can be nested.',
  },
  {
    id: 'shared',
    img: 'https://public.getlasco.app/user_list.png',
    title: 'Share with loved ones',
    desc: 'Add multiple users to your library and build memories together.',
  },
  {
    id: 'encrypted',
    img: 'https://public.getlasco.app/screen_album2.webp',
    title: 'Encrypted on your device',
    desc: 'Everything is encrypted client-side before it leaves. Servers store only ciphertext.',
  },
];

function ScreenshotScrollSection() {
  const [activeIdx, setActiveIdx] = React.useState(0);
  const [scrollProgress, setScrollProgress] = React.useState(0);
  const [storageOpacity, setStorageOpacity] = React.useState(0);
  const panelRefs = React.useRef<(HTMLDivElement | null)[]>([]);
  const sectionRef = React.useRef<HTMLElement>(null);

  React.useEffect(() => {
    const obs = new IntersectionObserver(entries => {
      for (const e of entries) {
        if (e.isIntersecting) {
          const idx = Number(e.target.getAttribute('data-idx'));
          setActiveIdx(idx);
        }
      }
    }, {threshold: 0.5});
    for (const ref of panelRefs.current) {
      if (ref) obs.observe(ref);
    }
    return () => obs.disconnect();
  }, []);

  React.useEffect(() => {
    const section = sectionRef.current;
    if (!section) return;
    const handleScroll = () => {
      const sectionTop = section.getBoundingClientRect().top + window.scrollY;
      const progress = Math.min(Math.max((window.scrollY - sectionTop) / 400, 0), 1);
      setScrollProgress(progress);

      const timelinePanel = panelRefs.current[1];
      if (timelinePanel) {
        const rect = timelinePanel.getBoundingClientRect();
        const panelCenter = rect.top + rect.height / 2;
        const viewportCenter = window.innerHeight / 2;
        const distance = Math.abs(panelCenter - viewportCenter);
        const signedDistance = panelCenter - viewportCenter;
        const fadeRange = signedDistance > 0
          ? window.innerHeight * 0.5
          : window.innerHeight * 0.2;
        const t = Math.max(0, 1 - Math.abs(signedDistance) / fadeRange);
        setStorageOpacity(t * t * (3 - 2 * t));
      }
    };
    window.addEventListener('scroll', handleScroll, {passive: true});
    handleScroll();
    return () => window.removeEventListener('scroll', handleScroll);
  }, []);

  const scale = 1.1 - scrollProgress * 0.1;
  const tx = (1 - scrollProgress) * 25;
  const imgTransform = `translateX(${tx}vw) scale(${scale})`;
  // Only mount the storage image while it is fading in on the timeline panel.
  // When it is absent the screenshot is the stage's only child, so the flex
  // centering lands it in the true middle of the left column at every width.
  const storageVisible = storageOpacity > 0.05;

  return (
    <section className={styles.screenshotScrollSection} ref={sectionRef}>
      <div className={styles.mascotFlamingoWrap}>
        <img
          src="https://public.getlasco.app/mascot_laying_0_5x.png"
          aria-hidden="true"
          className={styles.mascotFlamingo}
        />
      </div>
      <div className={styles.screenshotScrollWrap}>
        <div className={styles.screenshotScrollLeft}>
          <div className={styles.screenshotStage}>
            <img
              src={SCREENSHOT_PANELS[activeIdx].img}
              alt={SCREENSHOT_PANELS[activeIdx].title}
              className={styles.screenshotScrollImg}
              style={{transform: imgTransform, transformOrigin: 'top center'}}
            />
            {storageVisible && (
              <img
                src="/img/storage.png"
                aria-hidden="true"
                className={styles.stickyStorage}
                style={{opacity: storageOpacity}}
              />
            )}
          </div>
        </div>
        <div className={styles.screenshotScrollRight}>
          {SCREENSHOT_PANELS.map(({id, title, desc, img}, idx) => (
            <div
              key={id}
              className={`${styles.screenshotScrollPanel} ${idx === 0 ? styles.screenshotScrollPanelFirst : ''}`}
              data-idx={idx}
              ref={el => { panelRefs.current[idx] = el; }}
            >
              <div className={styles.mobilePanelImages}>
                <img src={img} alt={title || 'Lasco screenshot'} className={styles.mobilePanelScreenshot} />
                {idx === 1 && (
                  <img src="/img/storage.png" aria-hidden="true" className={styles.mobilePanelStorage} />
                )}
              </div>
              <div className={styles.panelBottom}>
                {title && <h2 className={styles.featureTitle}>{title}</h2>}
                {idx === 1 && (
                  <img
                    src="https://public.getlasco.app/mascot_cloud.png"
                    aria-hidden="true"
                    className={styles.panelMascot}
                  />
                )}
                {idx === 2 && (
                  <img
                    src="https://public.getlasco.app/mascot_album.png"
                    aria-hidden="true"
                    className={styles.panelMascot}
                  />
                )}
                {idx === 3 && (
                  <img
                    src="https://public.getlasco.app/mascot_love.png"
                    aria-hidden="true"
                    className={styles.panelMascot}
                  />
                )}
                {idx === 4 && (
                  <img
                    src="https://public.getlasco.app/mascot_encrypted_0_5x.png"
                    aria-hidden="true"
                    className={styles.panelMascot}
                  />
                )}
              </div>
              {desc && (
                <p className={`${styles.featureDesc} ${idx === 0 ? styles.introDesc : ''}`}>
                  {desc}
                </p>
              )}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

// =============================================
// Why Lasco Stands Out Section
// =============================================

const WHY_ITEMS = [
  {
    id: 'no-server',
    title: 'No server to self-host',
    desc: 'Lasco syncs directly to file servers, NAS, or cloud storage. No backend to deploy or maintain.',
  },
  {
    id: 'sync-primitives',
    title: 'One sync model for everything',
    desc: 'The same primitives push to your cloud provider and back up to a local hard drive.',
  },
  {
    id: 'multi-device',
    title: 'Multi-device',
    desc: 'Lasco uses CRDTs so edits from every device merge nicely.',
  },
  {
    id: 'open-source',
    title: 'Open source',
    desc: 'Licensed under GPLv3.',
  },
  {
    id: 'native',
    title: 'Native apps',
    desc: 'Built natively for iOS and Android.',
  },
  {
    id: 'e2ee',
    title: 'E2EE',
    desc: 'Your photos are encrypted on your device before they leave.',
  },
];

function WhyLascoSection() {
  return (
    <section className={styles.whySection}>
      <div className="container">
        <h2 className={styles.whySectionTitle}>What makes Lasco different</h2>
        <ul className={styles.whyList}>
          {WHY_ITEMS.map(({ id, title, desc }) => (
            <li key={id} className={styles.whyItem}>
              <span className={styles.whyItemTitle}>{title}</span>
              <span className={styles.whyItemDesc}>{desc}</span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

// =============================================
// Waitlist Section
// =============================================

function WaitlistSection() {
  return (
    <section className={styles.waitlistSection}>
      <div className="container">
        <h2 className={styles.waitlistTitle}>Join the waitlist!</h2>
        <p style={{textAlign: 'center', fontFamily: "'Inter', sans-serif", fontSize: 18, color: 'rgb(0,0,0)', marginBottom: 16}}>TestFlight beta coming mid-June</p>
        <style>{`@import url('https://fonts.googleapis.com/css2?family=Inter&display=swap');`}</style>
        <div className="newsletter-form-container" style={{display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', width: '100%'}}>
          <form
            className="newsletter-form"
            action="https://app.loops.so/api/newsletter-form/cmq5ajyf1031m0i5ewdemcii3"
            method="POST"
            style={{display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', width: '100%'}}
          >
            <input
              className="newsletter-form-input"
              placeholder="you@example.com"
              required
              style={{fontFamily: "'Inter', sans-serif", color: 'rgb(0, 0, 0)', fontSize: 14, margin: '0px 0px 10px', width: '100%', maxWidth: 300, minWidth: 100, background: 'rgb(255, 255, 255)', border: '1px solid rgb(209, 213, 219)', boxSizing: 'border-box', boxShadow: 'rgba(0, 0, 0, 0.05) 0px 1px 2px', borderRadius: 6, padding: '8px 12px'}}
              type="email"
              name="newsletter-form-input"
            />
            <button
              type="submit"
              className="newsletter-form-button"
              style={{background: 'rgb(225, 145, 234)', fontSize: 18, color: 'rgb(255, 255, 255)', fontFamily: "'Inter', sans-serif", display: 'flex', width: '100%', maxWidth: 300, whiteSpace: 'normal', height: 38, alignItems: 'center', justifyContent: 'center', flexDirection: 'row', padding: '9px 17px', boxShadow: 'rgba(0, 0, 0, 0.05) 0px 1px 2px', borderRadius: 6, textAlign: 'center', fontStyle: 'normal', fontWeight: 500, lineHeight: '20px', border: 'none', cursor: 'pointer'}}
            >Join Waitlist</button>
            <button
              type="button"
              className="newsletter-loading-button"
              style={{background: 'rgb(225, 145, 234)', fontSize: 18, color: 'rgb(255, 255, 255)', fontFamily: "'Inter', sans-serif", display: 'none', width: '100%', maxWidth: 300, whiteSpace: 'normal', height: 38, alignItems: 'center', justifyContent: 'center', flexDirection: 'row', padding: '9px 17px', boxShadow: 'rgba(0, 0, 0, 0.05) 0px 1px 2px', borderRadius: 6, textAlign: 'center', fontStyle: 'normal', fontWeight: 500, lineHeight: '20px', border: 'none', cursor: 'pointer'}}
            >Please wait...</button>
          </form>
          <div className="newsletter-success" style={{display: 'none', alignItems: 'center', justifyContent: 'center', width: '100%'}}>
            <p className="newsletter-success-message" style={{fontFamily: "'Inter', sans-serif", color: 'rgb(0, 0, 0)', fontSize: 14}}>Thanks! We'll be in touch!</p>
          </div>
          <div className="newsletter-error" style={{display: 'none', alignItems: 'center', justifyContent: 'center', width: '100%'}}>
            <p className="newsletter-error-message" style={{fontFamily: "'Inter', sans-serif", color: 'rgb(185, 28, 28)', fontSize: 14}}>Oops! Something went wrong, please try again</p>
          </div>
          <button
            className="newsletter-back-button"
            type="button"
            style={{color: '#6b7280', font: '14px Inter, sans-serif', margin: '10px auto', textAlign: 'center', display: 'none', background: 'transparent', border: 'none', cursor: 'pointer'}}
          >← Back</button>
        </div>
        <script dangerouslySetInnerHTML={{__html: `
function submitHandler(event) {
  event.preventDefault();
  var container = event.target.parentNode;
  var form = container.querySelector(".newsletter-form");
  var formInput = container.querySelector(".newsletter-form-input");
  var success = container.querySelector(".newsletter-success");
  var errorContainer = container.querySelector(".newsletter-error");
  var errorMessage = container.querySelector(".newsletter-error-message");
  var backButton = container.querySelector(".newsletter-back-button");
  var submitButton = container.querySelector(".newsletter-form-button");
  var loadingButton = container.querySelector(".newsletter-loading-button");
  const rateLimit = () => {
    errorContainer.style.display = "flex";
    errorMessage.innerText = "Too many signups, please try again in a little while";
    submitButton.style.display = "none";
    formInput.style.display = "none";
    backButton.style.display = "block";
  };
  var time = new Date();
  var timestamp = time.valueOf();
  var previousTimestamp = localStorage.getItem("loops-form-timestamp");
  if (previousTimestamp && Number(previousTimestamp) + 60000 > timestamp) { rateLimit(); return; }
  localStorage.setItem("loops-form-timestamp", timestamp);
  submitButton.style.display = "none";
  loadingButton.style.display = "flex";
  var formBody = "userGroup=waitlist&mailingLists=&email=" + encodeURIComponent(formInput.value);
  fetch(event.target.action, { method: "POST", body: formBody, headers: { "Content-Type": "application/x-www-form-urlencoded" } })
    .then((res) => [res.ok, res.json(), res])
    .then(([ok, dataPromise, res]) => {
      if (ok) { success.style.display = "flex"; form.reset(); }
      else { dataPromise.then(data => { errorContainer.style.display = "flex"; errorMessage.innerText = data.message ? data.message : res.statusText; }); }
    })
    .catch(error => {
      if (error.message === "Failed to fetch") { rateLimit(); return; }
      errorContainer.style.display = "flex";
      if (error.message) errorMessage.innerText = error.message;
      localStorage.setItem("loops-form-timestamp", '');
    })
    .finally(() => { formInput.style.display = "none"; loadingButton.style.display = "none"; backButton.style.display = "block"; });
}
function resetFormHandler(event) {
  var container = event.target.parentNode;
  var formInput = container.querySelector(".newsletter-form-input");
  var success = container.querySelector(".newsletter-success");
  var errorContainer = container.querySelector(".newsletter-error");
  var errorMessage = container.querySelector(".newsletter-error-message");
  var backButton = container.querySelector(".newsletter-back-button");
  var submitButton = container.querySelector(".newsletter-form-button");
  success.style.display = "none";
  errorContainer.style.display = "none";
  errorMessage.innerText = "Oops! Something went wrong, please try again";
  backButton.style.display = "none";
  formInput.style.display = "flex";
  submitButton.style.display = "flex";
}
var formContainers = document.getElementsByClassName("newsletter-form-container");
for (var i = 0; i < formContainers.length; i++) {
  var formContainer = formContainers[i];
  var handlersAdded = formContainer.classList.contains('newsletter-handlers-added');
  if (handlersAdded) continue;
  formContainer.querySelector(".newsletter-form").addEventListener("submit", submitHandler);
  formContainer.querySelector(".newsletter-back-button").addEventListener("click", resetFormHandler);
  formContainer.classList.add("newsletter-handlers-added");
}
        `}} />
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={siteConfig.title}
      description="Keep your memories usable, safe and private.">
      <main>
        <h1 className={styles.heroTitle}>Self (non hosted) photo management app</h1>
        <p className={styles.mobileIntroText}>{SCREENSHOT_PANELS[0].desc}</p>
        <div style={{width: '100%', aspectRatio: '1448/360', display: 'block'}} />
        <ScreenshotScrollSection />
        <WhyLascoSection />
        <div style={{display: 'flex', justifyContent: 'center', padding: '120px 0 200px'}}>
          <img
            src="https://public.getlasco.app/mascot_hole.png"
            aria-hidden="true"
            style={{height: 200, maxWidth: '100%'}}
          />
        </div>
        {/* <WaitlistSection /> */}
      </main>
    </Layout>
  );
}
