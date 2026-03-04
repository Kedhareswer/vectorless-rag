import { type LucideIcon } from 'lucide-react';
import clsx from 'clsx';
import styles from './IconButton.module.css';

interface IconButtonProps {
  icon: LucideIcon;
  onClick?: () => void;
  title?: string;
  className?: string;
  variant?: 'ghost' | 'default';
  size?: 'sm' | 'md';
  disabled?: boolean;
}

export function IconButton({
  icon: Icon,
  onClick,
  title,
  className,
  variant = 'ghost',
  size = 'md',
  disabled = false,
}: IconButtonProps) {
  const iconSize = size === 'sm' ? 16 : 20;

  return (
    <button
      className={clsx(
        styles.button,
        styles[variant],
        styles[size],
        className
      )}
      onClick={onClick}
      title={title}
      disabled={disabled}
      type="button"
    >
      <Icon size={iconSize} />
    </button>
  );
}
