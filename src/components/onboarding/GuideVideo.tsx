import styles from './Onboarding.module.css';

function prefersReducedMotion(): boolean {
  return typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

interface GuideVideoProps {
  src: string;
  label: string;
}

/** Silent looping screen recording; waits for a click when the user prefers reduced motion. */
export function GuideVideo({ src, label }: GuideVideoProps) {
  const still = prefersReducedMotion();
  return (
    <video
      key={src}
      className={styles.video}
      src={src}
      aria-label={label}
      autoPlay={!still}
      loop
      muted
      playsInline
      controls={still}
      preload="metadata"
    />
  );
}
