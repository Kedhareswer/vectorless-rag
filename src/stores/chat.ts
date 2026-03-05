import { create } from 'zustand';
import {
  listConversations,
  getConversationMessages,
  saveConversationIPC,
  saveMessageIPC,
  deleteConversationIPC,
  getTraces,
  getSteps,
} from '../lib/tauri';
import { useDocumentsStore } from './documents';

export interface Conversation {
  id: string;
  title: string;
  docId: string | null;
  createdAt: string;
}

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  createdAt: string;
}

export interface ExplorationStep {
  stepNumber: number;
  tool: string;
  inputSummary: string;
  outputSummary: string;
  tokensUsed: number;
  latencyMs: number;
  /** Cost in $ for this step, computed by the backend using per-model input/output rates */
  cost: number;
  status: 'running' | 'complete';
  /** Node IDs visited by this step (Feature 4: Live Visualization) */
  nodeIds?: string[];
}

export interface SessionTotals {
  tokens: number;
  cost: number;
  latency: number;
  steps: number;
}

interface ChatState {
  conversations: Conversation[];
  activeConversationId: string | null;
  messages: ChatMessage[];
  explorationSteps: ExplorationStep[];
  isExploring: boolean;
  /** Accumulated set of all visited node IDs (Feature 4) */
  visitedNodeIds: string[];
  /** Currently active node being explored (Feature 4) */
  activeNodeId: string | null;
  /** Cumulative session totals from previous queries in this conversation */
  sessionTotals: SessionTotals;
  /** Steps from previous queries loaded from DB (for session timeline view) */
  sessionSteps: ExplorationStep[];
  /** Whether session totals are being loaded from DB */
  isLoadingSession: boolean;

  createConversation: (title: string, docId?: string) => string;
  setActiveConversation: (id: string | null) => void;
  addMessage: (message: ChatMessage) => void;
  addExplorationStep: (step: ExplorationStep) => void;
  updateStepStatus: (stepNumber: number, status: ExplorationStep['status'], outputSummary?: string, nodeIds?: string[], tokensUsed?: number, latencyMs?: number, cost?: number) => void;
  setIsExploring: (exploring: boolean) => void;
  clearSteps: () => void;
  loadConversations: () => Promise<void>;
  loadMessages: (convId: string) => Promise<void>;
  deleteConversation: (convId: string) => Promise<void>;
  loadSessionTotals: (convId: string) => Promise<void>;
}

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export const useChatStore = create<ChatState>((set, get) => ({
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

  createConversation: (title: string, docId?: string) => {
    const id = generateId();
    const conversation: Conversation = {
      id,
      title,
      docId: docId ?? null,
      createdAt: new Date().toISOString(),
    };
    set((state) => ({
      conversations: [conversation, ...state.conversations],
      activeConversationId: id,
      messages: [],
      explorationSteps: [],
      visitedNodeIds: [],
      activeNodeId: null,
      sessionTotals: { tokens: 0, cost: 0, latency: 0, steps: 0 },
      sessionSteps: [],
    }));
    // Persist to backend (fire-and-forget)
    saveConversationIPC(id, title, docId ?? null).catch((err) =>
      console.warn('Failed to save conversation:', err)
    );
    return id;
  },

  setActiveConversation: (id: string | null) => {
    set({
      activeConversationId: id,
      messages: [],
      explorationSteps: [],
      visitedNodeIds: [],
      activeNodeId: null,
      sessionTotals: { tokens: 0, cost: 0, latency: 0, steps: 0 },
      sessionSteps: [],
      isLoadingSession: !!id,
    });

    // Restore document association from conversation
    const conv = id ? get().conversations.find((c) => c.id === id) : null;
    useDocumentsStore.getState().setActiveDocument(conv?.docId ?? null);

    if (id) {
      get().loadMessages(id);
      get().loadSessionTotals(id);
    }
  },

  addMessage: (message: ChatMessage) => {
    set((state) => ({
      messages: [...state.messages, message],
    }));
    // Persist to backend
    const convId = get().activeConversationId;
    if (convId) {
      saveMessageIPC(message.id, convId, message.role, message.content).catch((err) =>
        console.warn('Failed to save message:', err)
      );
    }
  },

  addExplorationStep: (step: ExplorationStep) => {
    set((state) => ({
      explorationSteps: [...state.explorationSteps, step],
      activeNodeId: step.nodeIds?.[0] ?? state.activeNodeId,
    }));
  },

  updateStepStatus: (stepNumber: number, status: ExplorationStep['status'], outputSummary?: string, nodeIds?: string[], tokensUsed?: number, latencyMs?: number, cost?: number) => {
    set((state) => {
      const newVisited = nodeIds
        ? [...state.visitedNodeIds, ...nodeIds.filter((id) => !state.visitedNodeIds.includes(id))]
        : state.visitedNodeIds;

      return {
        explorationSteps: state.explorationSteps.map((step) =>
          step.stepNumber === stepNumber
            ? {
                ...step,
                status,
                ...(outputSummary !== undefined ? { outputSummary } : {}),
                ...(nodeIds ? { nodeIds } : {}),
                ...(tokensUsed !== undefined ? { tokensUsed } : {}),
                ...(latencyMs !== undefined ? { latencyMs } : {}),
                ...(cost !== undefined ? { cost } : {}),
              }
            : step
        ),
        visitedNodeIds: newVisited,
        activeNodeId: status === 'complete' ? null : state.activeNodeId,
      };
    });
  },

  setIsExploring: (exploring: boolean) => {
    set({
      isExploring: exploring,
      ...(exploring ? {} : { activeNodeId: null }),
    });
  },

  clearSteps: () => {
    // Accumulate current query totals and steps into session before clearing
    const { explorationSteps, sessionTotals, sessionSteps } = get();
    let queryTokens = 0;
    let queryCost = 0;
    let queryLatency = 0;
    for (const step of explorationSteps) {
      queryTokens += step.tokensUsed;
      queryCost += step.cost;
      queryLatency += step.latencyMs;
    }
    set({
      explorationSteps: [],
      visitedNodeIds: [],
      activeNodeId: null,
      sessionTotals: {
        tokens: sessionTotals.tokens + queryTokens,
        cost: sessionTotals.cost + queryCost,
        latency: sessionTotals.latency + queryLatency,
        steps: sessionTotals.steps + explorationSteps.length,
      },
      sessionSteps: [...sessionSteps, ...explorationSteps],
    });
  },

  loadConversations: async () => {
    try {
      const records = await listConversations();
      const conversations: Conversation[] = records.map((r) => ({
        id: r.id,
        title: r.title,
        docId: r.doc_id,
        createdAt: r.created_at,
      }));
      set({ conversations });
    } catch (err) {
      console.warn('Failed to load conversations:', err);
    }
  },

  loadMessages: async (convId: string) => {
    try {
      const records = await getConversationMessages(convId);
      // Guard against stale results: only apply if this conversation is still active
      if (get().activeConversationId !== convId) return;
      const messages: ChatMessage[] = records.map((r) => ({
        id: r.id,
        role: r.role as 'user' | 'assistant',
        content: r.content,
        createdAt: r.created_at,
      }));
      set({ messages });
    } catch (err) {
      console.warn('Failed to load messages:', err);
    }
  },

  deleteConversation: async (convId: string) => {
    try {
      await deleteConversationIPC(convId);
      set((state) => ({
        conversations: state.conversations.filter((c) => c.id !== convId),
        ...(state.activeConversationId === convId
          ? { activeConversationId: null, messages: [], explorationSteps: [], visitedNodeIds: [], activeNodeId: null, sessionTotals: { tokens: 0, cost: 0, latency: 0, steps: 0 }, sessionSteps: [] }
          : {}),
      }));
    } catch (err) {
      console.warn('Failed to delete conversation:', err);
    }
  },

  loadSessionTotals: async (convId: string) => {
    try {
      const traces = await getTraces(convId);
      let tokens = 0;
      let cost = 0;
      let latency = 0;
      let steps = 0;
      for (const t of traces) {
        tokens += t.total_tokens;
        cost += t.total_cost;
        latency += t.total_latency_ms;
        steps += t.steps_count;
      }

      // Load historical steps from all traces
      const allSteps: ExplorationStep[] = [];
      let globalStepNum = 0;
      // traces are DESC by created_at, reverse to get chronological order
      for (const t of [...traces].reverse()) {
        try {
          const dbSteps = await getSteps(t.id);
          for (const s of dbSteps) {
            globalStepNum++;
            allSteps.push({
              stepNumber: globalStepNum,
              tool: s.tool_name,
              inputSummary: s.input_json,
              outputSummary: s.output_json,
              tokensUsed: s.tokens_used,
              latencyMs: s.latency_ms,
              cost: 0,
              status: 'complete',
            });
          }
        } catch {
          // Skip traces whose steps fail to load
        }
      }

      // Guard against stale results
      if (get().activeConversationId !== convId) return;
      set({
        sessionTotals: { tokens, cost, latency, steps },
        sessionSteps: allSteps,
        isLoadingSession: false,
      });
    } catch (err) {
      console.warn('Failed to load session totals:', err);
      set({ isLoadingSession: false });
    }
  },
}));
