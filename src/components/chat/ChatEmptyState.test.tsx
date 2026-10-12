import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChatEmptyState, starterQuestions } from './ChatEmptyState';

const props = (overrides = {}) => ({
  hasProvider: false,
  docNames: [] as string[],
  isIngesting: false,
  onAddModel: vi.fn(),
  onUseOffline: vi.fn(),
  onAddDocument: vi.fn(),
  onFocusInput: vi.fn(),
  onAsk: vi.fn(),
  ...overrides,
});

describe('ChatEmptyState', () => {
  it('starts by adding a model, and locks later steps', () => {
    const p = props();
    render(<ChatEmptyState {...p} />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a model' }));
    expect(p.onAddModel).toHaveBeenCalled();
    expect(screen.getByRole('button', { name: /Add document/ })).toBeDisabled();
  });

  it('offers a model the user already runs locally, without a download', () => {
    const onUseLocalServer = vi.fn();
    const p = props({ localServer: { label: 'Ollama', model: 'llama3.2:3b' }, onUseLocalServer });
    render(<ChatEmptyState {...p} />);
    expect(screen.getByText(/answer with llama3.2:3b, no download needed/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Use Ollama model' }));
    expect(onUseLocalServer).toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Other options' }));
    expect(p.onAddModel).toHaveBeenCalled();
  });

  it('asks for the download when the offline provider has no model yet', () => {
    const p = props({ localModelMissing: true });
    render(<ChatEmptyState {...p} />);
    expect(screen.getByText('Download the offline model')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Download' }));
    expect(p.onUseOffline).toHaveBeenCalled();
  });

  it('moves on to adding a document once a provider exists', () => {
    const p = props({ hasProvider: true });
    render(<ChatEmptyState {...p} />);
    expect(screen.queryByRole('button', { name: 'Add a model' })).not.toBeInTheDocument();
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
