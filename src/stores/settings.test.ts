import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useSettingsStore, type ProviderConfig } from './settings';

vi.mock('../lib/tauri', () => ({
  getProviders: vi.fn().mockResolvedValue([]),
  saveProvider: vi.fn().mockResolvedValue(undefined),
  deleteProvider: vi.fn().mockResolvedValue(undefined),
}));

const initialState = () => ({
  providers: [],
  activeProviderId: null,
  maxExplorationSteps: 10,
  isLoading: false,
  error: null,
});

describe('SettingsStore', () => {
  beforeEach(() => {
    useSettingsStore.setState(initialState());
  });

  // ── Initial state ──────────────────────────────────────────────

  it('has correct initial state', () => {
    const state = useSettingsStore.getState();
    expect(state.providers).toEqual([]);
    expect(state.activeProviderId).toBeNull();
    expect(state.maxExplorationSteps).toBe(10);
    expect(state.isLoading).toBe(false);
    expect(state.error).toBeNull();
  });

  // ── setProviders ───────────────────────────────────────────────

  it('setProviders replaces providers and detects active', () => {
    const providers = [
      makeProvider('p1', 'groq', false),
      makeProvider('p2', 'openai', true),
    ];

    useSettingsStore.getState().setProviders(providers);

    const state = useSettingsStore.getState();
    expect(state.providers).toEqual(providers);
    expect(state.activeProviderId).toBe('p2');
  });

  it('setProviders sets activeProviderId to null when no active provider', () => {
    const providers = [
      makeProvider('p1', 'groq', false),
      makeProvider('p2', 'openai', false),
    ];

    useSettingsStore.getState().setProviders(providers);

    expect(useSettingsStore.getState().activeProviderId).toBeNull();
  });

  // ── addProvider ────────────────────────────────────────────────

  it('addProvider appends a provider', () => {
    useSettingsStore.setState({ providers: [makeProvider('p1', 'groq', true)] });

    useSettingsStore.getState().addProvider(makeProvider('p2', 'openai', false));

    const providers = useSettingsStore.getState().providers;
    expect(providers).toHaveLength(2);
    expect(providers[1].id).toBe('p2');
  });

  // ── updateProvider ─────────────────────────────────────────────

  it('updateProvider merges partial updates into matching provider', () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true)],
    });

    useSettingsStore.getState().updateProvider('p1', { model: 'llama-3.3-70b', apiKey: 'new-key' });

    const provider = useSettingsStore.getState().providers[0];
    expect(provider.model).toBe('llama-3.3-70b');
    expect(provider.apiKey).toBe('new-key');
    // Other fields should be preserved
    expect(provider.name).toBe('groq');
    expect(provider.isActive).toBe(true);
  });

  it('updateProvider does not affect other providers', () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true), makeProvider('p2', 'openai', false)],
    });

    useSettingsStore.getState().updateProvider('p1', { model: 'updated' });

    const p2 = useSettingsStore.getState().providers[1];
    expect(p2.model).toBe('default-model');
  });

  // ── removeProvider ─────────────────────────────────────────────

  it('removeProvider filters out the provider', () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true), makeProvider('p2', 'openai', false)],
      activeProviderId: 'p1',
    });

    useSettingsStore.getState().removeProvider('p2');

    const state = useSettingsStore.getState();
    expect(state.providers).toHaveLength(1);
    expect(state.providers[0].id).toBe('p1');
    expect(state.activeProviderId).toBe('p1');
  });

  it('removeProvider clears activeProviderId when removing active provider', () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true)],
      activeProviderId: 'p1',
    });

    useSettingsStore.getState().removeProvider('p1');

    const state = useSettingsStore.getState();
    expect(state.providers).toEqual([]);
    expect(state.activeProviderId).toBeNull();
  });

  // ── setActiveProvider ──────────────────────────────────────────

  it('setActiveProvider updates activeProviderId and toggles isActive on providers', () => {
    useSettingsStore.setState({
      providers: [
        makeProvider('p1', 'groq', true),
        makeProvider('p2', 'openai', false),
      ],
      activeProviderId: 'p1',
    });

    useSettingsStore.getState().setActiveProvider('p2');

    const state = useSettingsStore.getState();
    expect(state.activeProviderId).toBe('p2');
    expect(state.providers.find((p) => p.id === 'p1')!.isActive).toBe(false);
    expect(state.providers.find((p) => p.id === 'p2')!.isActive).toBe(true);
  });

  it('setActiveProvider with null deactivates all providers', () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true)],
      activeProviderId: 'p1',
    });

    useSettingsStore.getState().setActiveProvider(null);

    const state = useSettingsStore.getState();
    expect(state.activeProviderId).toBeNull();
    expect(state.providers[0].isActive).toBe(false);
  });

  // ── setMaxSteps ────────────────────────────────────────────────

  it('setMaxSteps updates maxExplorationSteps', () => {
    useSettingsStore.getState().setMaxSteps(25);
    expect(useSettingsStore.getState().maxExplorationSteps).toBe(25);
  });

  it('setMaxSteps can set to 1', () => {
    useSettingsStore.getState().setMaxSteps(1);
    expect(useSettingsStore.getState().maxExplorationSteps).toBe(1);
  });

  // ── clearError ─────────────────────────────────────────────────

  it('clearError resets error to null', () => {
    useSettingsStore.setState({ error: 'Connection failed' });

    useSettingsStore.getState().clearError();

    expect(useSettingsStore.getState().error).toBeNull();
  });

  // ── loadProviders ──────────────────────────────────────────────

  it('loadProviders populates providers from backend', async () => {
    const { getProviders } = await import('../lib/tauri');
    vi.mocked(getProviders).mockResolvedValueOnce([
      { id: 'p1', name: 'groq', api_key: 'key-123', base_url: 'https://api.groq.com', model: 'llama-3', is_active: true },
    ]);

    await useSettingsStore.getState().loadProviders();

    const state = useSettingsStore.getState();
    expect(state.providers).toHaveLength(1);
    expect(state.providers[0].id).toBe('p1');
    expect(state.providers[0].apiKey).toBe('key-123');
    expect(state.providers[0].baseUrl).toBe('https://api.groq.com');
    expect(state.activeProviderId).toBe('p1');
    expect(state.isLoading).toBe(false);
  });

  it('loadProviders sets error on failure', async () => {
    const { getProviders } = await import('../lib/tauri');
    vi.mocked(getProviders).mockRejectedValueOnce(new Error('Network error'));

    await useSettingsStore.getState().loadProviders();

    const state = useSettingsStore.getState();
    expect(state.isLoading).toBe(false);
    expect(state.error).toBe('Error: Network error');
  });

  // ── saveProviderToBackend ──────────────────────────────────────

  it('saveProviderToBackend adds new provider to local state', async () => {
    const provider = makeProvider('p1', 'groq', true);

    await useSettingsStore.getState().saveProviderToBackend(provider);

    const state = useSettingsStore.getState();
    expect(state.providers).toHaveLength(1);
    expect(state.providers[0].id).toBe('p1');
    expect(state.activeProviderId).toBe('p1');
  });

  it('saveProviderToBackend updates existing provider in local state', async () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true)],
    });

    const updated = { ...makeProvider('p1', 'groq', true), model: 'mixtral-8x7b' };
    await useSettingsStore.getState().saveProviderToBackend(updated);

    const state = useSettingsStore.getState();
    expect(state.providers).toHaveLength(1);
    expect(state.providers[0].model).toBe('mixtral-8x7b');
  });

  it('saveProviderToBackend sets error on failure', async () => {
    const { saveProvider } = await import('../lib/tauri');
    vi.mocked(saveProvider).mockRejectedValueOnce(new Error('Save failed'));

    await useSettingsStore.getState().saveProviderToBackend(makeProvider('p1', 'groq', true));

    expect(useSettingsStore.getState().error).toBe('Error: Save failed');
  });

  // ── deleteProviderFromBackend ──────────────────────────────────

  it('deleteProviderFromBackend removes provider from local state', async () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true), makeProvider('p2', 'openai', false)],
      activeProviderId: 'p1',
    });

    await useSettingsStore.getState().deleteProviderFromBackend('p2');

    const state = useSettingsStore.getState();
    expect(state.providers).toHaveLength(1);
    expect(state.activeProviderId).toBe('p1');
  });

  it('deleteProviderFromBackend clears activeProviderId when deleting active', async () => {
    useSettingsStore.setState({
      providers: [makeProvider('p1', 'groq', true)],
      activeProviderId: 'p1',
    });

    await useSettingsStore.getState().deleteProviderFromBackend('p1');

    const state = useSettingsStore.getState();
    expect(state.providers).toEqual([]);
    expect(state.activeProviderId).toBeNull();
  });

  it('deleteProviderFromBackend sets error on failure', async () => {
    const { deleteProvider } = await import('../lib/tauri');
    vi.mocked(deleteProvider).mockRejectedValueOnce(new Error('Delete failed'));

    await useSettingsStore.getState().deleteProviderFromBackend('p1');

    expect(useSettingsStore.getState().error).toBe('Error: Delete failed');
  });
});

// ── Helpers ────────────────────────────────────────────────────────

function makeProvider(id: string, name: string, isActive: boolean): ProviderConfig {
  return {
    id,
    name,
    apiKey: 'test-key',
    baseUrl: 'https://api.example.com',
    model: 'default-model',
    isActive,
  };
}
