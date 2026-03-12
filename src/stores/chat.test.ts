import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useChatStore, type ChatMessage, type ExplorationStep } from './chat';

// Mock the tauri IPC module
vi.mock('../lib/tauri', () => ({
  listConversations: vi.fn().mockResolvedValue([]),
  getConversationMessages: vi.fn().mockResolvedValue([]),
  saveConversationIPC: vi.fn().mockResolvedValue(undefined),
  saveMessageIPC: vi.fn().mockResolvedValue(undefined),
  deleteConversationIPC: vi.fn().mockResolvedValue(undefined),
  getTraces: vi.fn().mockResolvedValue([]),
  getSteps: vi.fn().mockResolvedValue([]),
}));

// Mock the documents store used in setActiveConversation
vi.mock('./documents', () => ({
  useDocumentsStore: {
    getState: () => ({
      setActiveDocument: vi.fn(),
    }),
  },
}));

const initialState = () => ({
  conversations: [],
  activeConversationId: null,
  messages: [],
  explorationSteps: [],
  isExploring: false,
  visitedNodeIds: [],
  activeNodeId: null,
  sessionTotals: { tokens: 0, cost: 0, latency: 0, steps: 0 },
  sessionSteps: [],
  isLoadingSession: false,
});

describe('ChatStore', () => {
  beforeEach(() => {
    useChatStore.setState(initialState());
  });

  // ── Initial state ──────────────────────────────────────────────

  it('has correct initial state', () => {
    const state = useChatStore.getState();
    expect(state.conversations).toEqual([]);
    expect(state.activeConversationId).toBeNull();
    expect(state.messages).toEqual([]);
    expect(state.explorationSteps).toEqual([]);
    expect(state.isExploring).toBe(false);
    expect(state.visitedNodeIds).toEqual([]);
    expect(state.activeNodeId).toBeNull();
    expect(state.sessionTotals).toEqual({ tokens: 0, cost: 0, latency: 0, steps: 0 });
    expect(state.sessionSteps).toEqual([]);
    expect(state.isLoadingSession).toBe(false);
  });

  // ── createConversation ─────────────────────────────────────────

  it('createConversation adds a conversation and sets it active', () => {
    const { createConversation } = useChatStore.getState();
    const id = createConversation('Test Chat', 'doc-1');

    const state = useChatStore.getState();
    expect(state.conversations).toHaveLength(1);
    expect(state.conversations[0].title).toBe('Test Chat');
    expect(state.conversations[0].docId).toBe('doc-1');
    expect(state.activeConversationId).toBe(id);
    expect(state.messages).toEqual([]);
    expect(state.explorationSteps).toEqual([]);
  });

  it('createConversation sets docId to null when not provided', () => {
    const { createConversation } = useChatStore.getState();
    createConversation('No Doc Chat');

    const state = useChatStore.getState();
    expect(state.conversations[0].docId).toBeNull();
  });

  it('createConversation prepends to conversation list', () => {
    const { createConversation } = useChatStore.getState();
    createConversation('First');
    createConversation('Second');

    const state = useChatStore.getState();
    expect(state.conversations).toHaveLength(2);
    expect(state.conversations[0].title).toBe('Second');
    expect(state.conversations[1].title).toBe('First');
  });

  it('createConversation resets session totals and steps', () => {
    // Set some existing session state
    useChatStore.setState({
      sessionTotals: { tokens: 100, cost: 0.5, latency: 2000, steps: 5 },
      sessionSteps: [makeStep(1)],
      visitedNodeIds: ['n1', 'n2'],
    });

    const { createConversation } = useChatStore.getState();
    createConversation('Fresh Chat');

    const state = useChatStore.getState();
    expect(state.sessionTotals).toEqual({ tokens: 0, cost: 0, latency: 0, steps: 0 });
    expect(state.sessionSteps).toEqual([]);
    expect(state.visitedNodeIds).toEqual([]);
  });

  // ── setActiveConversation ──────────────────────────────────────

  it('setActiveConversation clears messages, steps, and sets loading', () => {
    useChatStore.setState({
      messages: [makeMessage('m1', 'user', 'hello')],
      explorationSteps: [makeStep(1)],
      visitedNodeIds: ['n1'],
      activeNodeId: 'n1',
    });

    const { setActiveConversation } = useChatStore.getState();
    setActiveConversation('conv-1');

    const state = useChatStore.getState();
    expect(state.activeConversationId).toBe('conv-1');
    expect(state.messages).toEqual([]);
    expect(state.explorationSteps).toEqual([]);
    expect(state.visitedNodeIds).toEqual([]);
    expect(state.activeNodeId).toBeNull();
    expect(state.isLoadingSession).toBe(true);
  });

  it('setActiveConversation with null clears everything without loading', () => {
    const { setActiveConversation } = useChatStore.getState();
    setActiveConversation(null);

    const state = useChatStore.getState();
    expect(state.activeConversationId).toBeNull();
    expect(state.isLoadingSession).toBe(false);
  });

  // ── addMessage ─────────────────────────────────────────────────

  it('addMessage appends a message to the list', () => {
    const { addMessage } = useChatStore.getState();
    const msg = makeMessage('m1', 'user', 'Hello');
    addMessage(msg);

    const state = useChatStore.getState();
    expect(state.messages).toHaveLength(1);
    expect(state.messages[0]).toEqual(msg);
  });

  it('addMessage preserves existing messages', () => {
    const { addMessage } = useChatStore.getState();
    addMessage(makeMessage('m1', 'user', 'Hello'));
    addMessage(makeMessage('m2', 'assistant', 'Hi there'));

    const state = useChatStore.getState();
    expect(state.messages).toHaveLength(2);
    expect(state.messages[0].id).toBe('m1');
    expect(state.messages[1].id).toBe('m2');
  });

  // ── addExplorationStep ─────────────────────────────────────────

  it('addExplorationStep appends a step', () => {
    const { addExplorationStep } = useChatStore.getState();
    const step = makeStep(1, { nodeIds: ['node-a'] });
    addExplorationStep(step);

    const state = useChatStore.getState();
    expect(state.explorationSteps).toHaveLength(1);
    expect(state.explorationSteps[0].stepNumber).toBe(1);
  });

  it('addExplorationStep sets activeNodeId from step nodeIds', () => {
    const { addExplorationStep } = useChatStore.getState();
    addExplorationStep(makeStep(1, { nodeIds: ['node-x', 'node-y'] }));

    expect(useChatStore.getState().activeNodeId).toBe('node-x');
  });

  it('addExplorationStep keeps existing activeNodeId when step has no nodeIds', () => {
    useChatStore.setState({ activeNodeId: 'existing' });

    const { addExplorationStep } = useChatStore.getState();
    addExplorationStep(makeStep(1));

    expect(useChatStore.getState().activeNodeId).toBe('existing');
  });

  // ── updateStepStatus ───────────────────────────────────────────

  it('updateStepStatus updates matching step', () => {
    useChatStore.setState({
      explorationSteps: [makeStep(1, { status: 'running' }), makeStep(2, { status: 'running' })],
    });

    const { updateStepStatus } = useChatStore.getState();
    updateStepStatus(1, 'complete', 'Done searching', ['n1', 'n2'], 150, 500, 0.01);

    const state = useChatStore.getState();
    const step = state.explorationSteps.find((s) => s.stepNumber === 1)!;
    expect(step.status).toBe('complete');
    expect(step.outputSummary).toBe('Done searching');
    expect(step.nodeIds).toEqual(['n1', 'n2']);
    expect(step.tokensUsed).toBe(150);
    expect(step.latencyMs).toBe(500);
    expect(step.cost).toBe(0.01);
  });

  it('updateStepStatus adds new nodeIds to visitedNodeIds without duplicates', () => {
    useChatStore.setState({
      explorationSteps: [makeStep(1, { status: 'running' })],
      visitedNodeIds: ['n1'],
    });

    const { updateStepStatus } = useChatStore.getState();
    updateStepStatus(1, 'complete', undefined, ['n1', 'n2', 'n3']);

    expect(useChatStore.getState().visitedNodeIds).toEqual(['n1', 'n2', 'n3']);
  });

  it('updateStepStatus clears activeNodeId on complete', () => {
    useChatStore.setState({
      explorationSteps: [makeStep(1, { status: 'running' })],
      activeNodeId: 'node-x',
    });

    const { updateStepStatus } = useChatStore.getState();
    updateStepStatus(1, 'complete');

    expect(useChatStore.getState().activeNodeId).toBeNull();
  });

  it('updateStepStatus keeps activeNodeId when status is not complete', () => {
    useChatStore.setState({
      explorationSteps: [makeStep(1, { status: 'running' })],
      activeNodeId: 'node-x',
    });

    const { updateStepStatus } = useChatStore.getState();
    updateStepStatus(1, 'running');

    expect(useChatStore.getState().activeNodeId).toBe('node-x');
  });

  it('updateStepStatus does not modify unmatched steps', () => {
    useChatStore.setState({
      explorationSteps: [makeStep(1, { status: 'running' }), makeStep(2, { status: 'running' })],
    });

    const { updateStepStatus } = useChatStore.getState();
    updateStepStatus(1, 'complete');

    const step2 = useChatStore.getState().explorationSteps.find((s) => s.stepNumber === 2)!;
    expect(step2.status).toBe('running');
  });

  // ── setIsExploring ─────────────────────────────────────────────

  it('setIsExploring sets the flag', () => {
    const { setIsExploring } = useChatStore.getState();
    setIsExploring(true);
    expect(useChatStore.getState().isExploring).toBe(true);

    setIsExploring(false);
    expect(useChatStore.getState().isExploring).toBe(false);
  });

  it('setIsExploring clears activeNodeId when exploration ends', () => {
    useChatStore.setState({ activeNodeId: 'node-1', isExploring: true });

    const { setIsExploring } = useChatStore.getState();
    setIsExploring(false);

    expect(useChatStore.getState().activeNodeId).toBeNull();
  });

  it('setIsExploring preserves activeNodeId when exploration starts', () => {
    useChatStore.setState({ activeNodeId: 'node-1' });

    const { setIsExploring } = useChatStore.getState();
    setIsExploring(true);

    expect(useChatStore.getState().activeNodeId).toBe('node-1');
  });

  // ── clearSteps ─────────────────────────────────────────────────

  it('clearSteps accumulates totals from current steps into session', () => {
    useChatStore.setState({
      explorationSteps: [
        makeStep(1, { tokensUsed: 100, cost: 0.01, latencyMs: 200 }),
        makeStep(2, { tokensUsed: 50, cost: 0.005, latencyMs: 100 }),
      ],
      sessionTotals: { tokens: 0, cost: 0, latency: 0, steps: 0 },
      sessionSteps: [],
    });

    const { clearSteps } = useChatStore.getState();
    clearSteps();

    const state = useChatStore.getState();
    expect(state.explorationSteps).toEqual([]);
    expect(state.sessionTotals).toEqual({
      tokens: 150,
      cost: 0.015,
      latency: 300,
      steps: 2,
    });
    expect(state.sessionSteps).toHaveLength(2);
  });

  it('clearSteps adds to existing session totals', () => {
    useChatStore.setState({
      explorationSteps: [makeStep(3, { tokensUsed: 50, cost: 0.01, latencyMs: 100 })],
      sessionTotals: { tokens: 200, cost: 0.05, latency: 1000, steps: 2 },
      sessionSteps: [makeStep(1), makeStep(2)],
    });

    const { clearSteps } = useChatStore.getState();
    clearSteps();

    const state = useChatStore.getState();
    expect(state.sessionTotals.tokens).toBe(250);
    expect(state.sessionTotals.cost).toBeCloseTo(0.06);
    expect(state.sessionTotals.latency).toBe(1100);
    expect(state.sessionTotals.steps).toBe(3);
    expect(state.sessionSteps).toHaveLength(3);
  });

  it('clearSteps resets visitedNodeIds and activeNodeId', () => {
    useChatStore.setState({
      explorationSteps: [makeStep(1)],
      visitedNodeIds: ['n1', 'n2'],
      activeNodeId: 'n1',
    });

    const { clearSteps } = useChatStore.getState();
    clearSteps();

    const state = useChatStore.getState();
    expect(state.visitedNodeIds).toEqual([]);
    expect(state.activeNodeId).toBeNull();
  });

  // ── deleteConversation ─────────────────────────────────────────

  it('deleteConversation removes conversation from list', async () => {
    useChatStore.setState({
      conversations: [
        { id: 'c1', title: 'Chat 1', docId: null, createdAt: '2025-01-01' },
        { id: 'c2', title: 'Chat 2', docId: null, createdAt: '2025-01-02' },
      ],
      activeConversationId: null,
    });

    await useChatStore.getState().deleteConversation('c1');

    const state = useChatStore.getState();
    expect(state.conversations).toHaveLength(1);
    expect(state.conversations[0].id).toBe('c2');
  });

  it('deleteConversation clears active state when deleting active conversation', async () => {
    useChatStore.setState({
      conversations: [{ id: 'c1', title: 'Chat 1', docId: null, createdAt: '2025-01-01' }],
      activeConversationId: 'c1',
      messages: [makeMessage('m1', 'user', 'hi')],
      explorationSteps: [makeStep(1)],
      visitedNodeIds: ['n1'],
      activeNodeId: 'n1',
    });

    await useChatStore.getState().deleteConversation('c1');

    const state = useChatStore.getState();
    expect(state.activeConversationId).toBeNull();
    expect(state.messages).toEqual([]);
    expect(state.explorationSteps).toEqual([]);
    expect(state.visitedNodeIds).toEqual([]);
  });

  it('deleteConversation does not clear active state when deleting non-active conversation', async () => {
    useChatStore.setState({
      conversations: [
        { id: 'c1', title: 'Chat 1', docId: null, createdAt: '2025-01-01' },
        { id: 'c2', title: 'Chat 2', docId: null, createdAt: '2025-01-02' },
      ],
      activeConversationId: 'c2',
      messages: [makeMessage('m1', 'user', 'hi')],
    });

    await useChatStore.getState().deleteConversation('c1');

    const state = useChatStore.getState();
    expect(state.activeConversationId).toBe('c2');
    expect(state.messages).toHaveLength(1);
  });
});

// ── Helpers ────────────────────────────────────────────────────────

function makeMessage(id: string, role: 'user' | 'assistant', content: string): ChatMessage {
  return { id, role, content, createdAt: '2025-01-01T00:00:00Z' };
}

function makeStep(stepNumber: number, overrides: Partial<ExplorationStep> = {}): ExplorationStep {
  return {
    stepNumber,
    tool: 'search_content',
    inputSummary: 'test input',
    outputSummary: 'test output',
    tokensUsed: 0,
    latencyMs: 0,
    cost: 0,
    status: 'complete',
    ...overrides,
  };
}
