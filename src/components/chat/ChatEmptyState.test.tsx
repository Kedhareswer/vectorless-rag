import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChatEmptyState, starterQuestions } from './ChatEmptyState';

const props = (overrides = {}) => ({
  hasProvider: false,
  docNames: [] as string[],
  isIngesting: false,
  onOpenSettings: vi.fn(),
  onAddDocument: vi.fn(),
  onFocusInput: vi.fn(),
  onAsk: vi.fn(),
  ...overrides,
});

describe('ChatEmptyState', () => {
  it('starts the checklist at the provider step and locks later steps', () => {
    const p = props();
    render(<ChatEmptyState {...p} />);
    fireEvent.click(screen.getByRole('button', { name: 'Open settings' }));
    expect(p.onOpenSettings).toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /Add document/ })).toBeDisabled();
  });

  it('moves on to adding a document once a provider exists', () => {
    const p = props({ hasProvider: true });
    render(<ChatEmptyState {...p} />);
    expect(screen.queryByRole('button', { name: 'Open settings' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Add document/ }));
    expect(p.onAddDocument).toHaveBeenCalled();
  });

  it('offers starter questions when ready, and sends the clicked one', () => {
    const p = props({ hasProvider: true, docNames: ['contract.pdf'] });
    render(<ChatEmptyState {...p} />);
    expect(screen.getByRole('heading', { name: 'Ask about contract.pdf' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Summarize the key points' }));
    expect(p.onAsk).toHaveBeenCalledWith('Summarize the key points');
  });

  it('adds a comparison starter only for several documents', () => {
    expect(starterQuestions(1).some((q) => q.startsWith('Compare'))).toBe(false);
    expect(starterQuestions(2).some((q) => q.startsWith('Compare'))).toBe(true);
  });
});
