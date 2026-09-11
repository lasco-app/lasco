import React, {type ReactNode} from 'react';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import DocusaurusLayout from '@theme/Layout';
import {Rive, Layout as RiveLayout, Fit, Alignment} from '@rive-app/canvas';

import styles from './index.module.css';

// =============================================
// Rive mascot animations (canvas runtime)
// =============================================

function RiveMascot({src, className}: {src: string; className?: string}) {
  const canvasRef = React.useRef<HTMLCanvasElement>(null);

  React.useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const riveInstance = new Rive({
      src,
      canvas,
      autoplay: true,
      layout: new RiveLayout({fit: Fit.Contain, alignment: Alignment.Center}),
      onLoad: () => riveInstance.resizeDrawingSurfaceToCanvas(),
    });
    const handleResize = () => riveInstance.resizeDrawingSurfaceToCanvas();
    window.addEventListener('resize', handleResize);
    return () => {
      window.removeEventListener('resize', handleResize);
      riveInstance.cleanup();
    };
  }, [src]);

  return <canvas ref={canvasRef} className={className} aria-hidden="true" />;
}

// =============================================
// Main sync visual
// =============================================

function StorageDiagram() {
  return (
    <section className={styles.storageDiagramSection} aria-label="Lasco connects directly to your storage">
      <div className="container">
        <img
          src="/img/mascot_main_anim2.png"
          alt="The Lasco mascot using the app to sync photos directly with a NAS, S3 storage, and an external drive"
          className={styles.mainSyncVisual}
        />
      </div>
    </section>
  );
}

function DownloadAppPrompt() {
  return (
    <section className={styles.downloadAppPrompt} aria-label="Download Lasco for Android">
      <div className="container">
        <div className={styles.downloadAppPromptInner}>
          <GooglePlayBadge />
        </div>
      </div>
    </section>
  );
}

function EasySyncSection() {
  return (
    <section className={styles.productStorySection} aria-labelledby="easy-sync-title">
      <div className="container">
        <h2 id="easy-sync-title" className={styles.productStoryTitle}>
          The easiest way to sync your photo library to storage you control
        </h2>
        <div className={styles.solutionComparison}>
          <div className={styles.solutionColumn}>
            <h3 className={styles.solutionLabel}>Existing solutions</h3>
            <ol className={styles.setupSteps}>
              <li>Follow a long tutorial to install a photo server on your NAS or home server.</li>
              <li>Set up Docker, storage paths, and configuration files.</li>
              <li>Keep the server and its services running.</li>
              <li>Handle updates and database migrations when they arrive.</li>
              <li>Set up and manage backups outside the photo app.</li>
            </ol>
          </div>
          <div className={`${styles.solutionColumn} ${styles.lascoSolutionColumn}`}>
            <h3 className={styles.solutionLabel}>With Lasco</h3>
            <ol className={styles.setupSteps}>
              <li>Download the app on your phone.</li>
              <li>Connect your NAS or S3 bucket.</li>
              <li>Sync.</li>
            </ol>
          </div>
        </div>
      </div>
    </section>
  );
}

function BackupRemoteSection() {
  return (
    <section className={`${styles.productStorySection} ${styles.backupSection}`} aria-labelledby="backup-remote-title">
      <div className="container">
        <h2 id="backup-remote-title" className={styles.productStoryTitle}>
          A backup is just another remote
        </h2>
        <div className={styles.backupContent}>
          <img
            src="/img/phone_ssd.png"
            alt="A phone connected directly to an external SSD"
            className={styles.backupVisual}
          />
          <div className={styles.productStoryCopy}>
            <p>Plug in a USB drive, add it as a remote, and sync.</p>
          </div>
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
    desc: 'Syncs directly from the app to the storage you control. No backend to deploy and maintain.',
  },
  {
    id: 'multi-device',
    title: 'Multi-device',
    desc: 'Changes from all your devices merge safely thanks to CRDT algorithms.',
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
  {
    id: 'multiple-users',
    title: 'Multiple users',
    desc: 'Add people to your library and build memories together.',
    screenshot: 'https://public.getlasco.app/user_list.png',
  },
];

function WhyLascoSection() {
  return (
    <section className={styles.whySection}>
      <div className="container">
        <ul className={styles.whyList}>
          {WHY_ITEMS.map(({ id, title, desc, screenshot }) => (
            <li key={id} className={styles.whyItem}>
              <span className={styles.whyItemTitle}>{title}</span>
              <span className={styles.whyItemDesc}>{desc}</span>
              {id === 'no-server' && (
                <ul className={styles.noServerRemoteList}>
                  <li>S3 bucket</li>
                  <li>NAS <span className={styles.comingSoon}>Coming soon</span></li>
                  <li>USB drive <span className={styles.comingSoon}>Coming soon</span></li>
                  <li>Lasco Cloud</li>
                </ul>
              )}
              {id === 'native' && (
                <img
                  src="/img/screen.webp"
                  alt="Lasco photo library on a phone"
                  className={styles.nativeAppScreenshot}
                />
              )}
              {id === 'multiple-users' && screenshot && (
                <img
                  src={screenshot}
                  alt="Lasco library member list"
                  className={styles.multipleUsersScreenshot}
                />
              )}
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

function GetStartedSection() {
  return (
    <section className={styles.getStartedSection} aria-labelledby="get-started-title">
      <div className="container">
        <div className={styles.getStartedCard}>
          <h2 id="get-started-title" className={styles.getStartedTitle}>Get started</h2>
          <ol className={styles.getStartedSteps}>
            <li className={styles.getStartedStep}>
              <span className={styles.getStartedMarker} aria-hidden="true" />
              <div>
                <h3>Download the app</h3>
                <GooglePlayBadge />
              </div>
            </li>
            <li className={styles.getStartedStep}>
              <span className={styles.getStartedMarker} aria-hidden="true" />
              <div>
                <h3>Set up a remote</h3>
                <ul>
                  <li>S3 bucket</li>
                  <li>NAS <span className={styles.comingSoon}>Coming soon</span></li>
                  <li>USB drive <span className={styles.comingSoon}>Coming soon</span></li>
                  <li>Lasco Cloud</li>
                </ul>
              </div>
            </li>
            <li className={styles.getStartedStep}>
              <span className={styles.getStartedMarker} aria-hidden="true" />
              <div>
                <h3>Import your library <span className={styles.optional}>Optional</span></h3>
                <p>On iOS, import iCloud or local photos through the app. On Android, import photos from your device.</p>
              </div>
            </li>
            <li className={styles.getStartedStep}>
              <span className={styles.getStartedMarker} aria-hidden="true" />
              <div>
                <h3>Keep using your library</h3>
                <p>Add new photos, organize them, sync, and back up.</p>
              </div>
            </li>
          </ol>
        </div>
      </div>
    </section>
  );
}

function GooglePlayBadge() {
  return (
    <a
      className={styles.googlePlayBadge}
      href="https://play.google.com/store/apps/details?id=com.lasco.lasco"
      target="_blank"
      rel="noreferrer"
      aria-label="Get Lasco on Google Play">
      <img src="/img/google-play-badge.svg" alt="Get it on Google Play" />
    </a>
  );
}

// =============================================
// Lasco Cloud Section
// =============================================

const CLOUD_PRICES = {
  usd: {monthly: '$2.99', annualMonthlyEquivalent: '$2.49'},
  eur: {monthly: '€2.99', annualMonthlyEquivalent: '€2.49'},
};

function LascoCloudSection() {
  const [currency, setCurrency] = React.useState<keyof typeof CLOUD_PRICES>('usd');
  const price = CLOUD_PRICES[currency];

  return (
    <section className={styles.cloudSection} aria-labelledby="lasco-cloud-title">
      <div className="container">
        <div className={styles.cloudCard}>
          <div className={styles.cloudCopy}>
            <h2 id="lasco-cloud-title" className={styles.cloudTitle}>Lasco Cloud</h2>
            <p className={styles.cloudDescription}>
              The easiest way to get started with Lasco.
            </p>
          </div>
          <div className={styles.cloudPlan}>
            <a className={styles.cloudCta} href="https://cloud.getlasco.app/subscribe-new">
              Get started
            </a>
            <div className={styles.cloudPlanDetails}>
              <div className={styles.currencyToggle} role="group" aria-label="Choose a display currency">
                <button
                  type="button"
                  className={`${styles.currencyOption} ${currency === 'usd' ? styles.currencyOptionActive : ''}`}
                  aria-pressed={currency === 'usd'}
                  onClick={() => setCurrency('usd')}>
                  USD
                </button>
                <button
                  type="button"
                  className={`${styles.currencyOption} ${currency === 'eur' ? styles.currencyOptionActive : ''}`}
                  aria-pressed={currency === 'eur'}
                  onClick={() => setCurrency('eur')}>
                  EUR
                </button>
              </div>
              <p className={styles.cloudPlanName}>50 GB</p>
              <div className={styles.cloudPricingOptions}>
                <div className={styles.cloudPricingOption}>
                  <p className={styles.cloudBillingPeriod}>Monthly</p>
                  <p className={styles.cloudPrice}>
                    {price.monthly}<span>/ month</span>
                  </p>
                </div>
                <div className={styles.cloudPricingOption}>
                  <p className={styles.cloudBillingPeriod}>Annual</p>
                  <p className={styles.cloudPrice}>
                    {price.annualMonthlyEquivalent}<span>/ month</span>
                  </p>
                  <p className={styles.cloudPriceNote}>Billed yearly</p>
                </div>
              </div>
              <ul className={styles.cloudFeatures}>
                <li>50 GB of encrypted photo storage</li>
                <li>Two remote copies of your library (2 × 50 GB)</li>
              </ul>
            </div>
          </div>
        </div>
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
    <DocusaurusLayout
      title={siteConfig.title}
      description="Keep your memories usable, safe and private.">
      <main>
        <h1 className={styles.heroTitle}>Private photo management. No server to deploy.</h1>
        <StorageDiagram />
        <DownloadAppPrompt />
        <EasySyncSection />
        <BackupRemoteSection />
        <WhyLascoSection />
        <GetStartedSection />
        <LascoCloudSection />
        <div style={{display: 'flex', justifyContent: 'center', padding: '120px 0 200px'}}>
          <RiveMascot
            src="/img/msacot_hole_anim.riv"
            className={styles.mascotHole}
          />
        </div>
        {/* <WaitlistSection /> */}
      </main>
    </DocusaurusLayout>
  );
}
