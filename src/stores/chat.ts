import { create } from 'zustand';
import {
  listConversations,
  getConversationMessages,
  saveConversationIPC,
  saveMessageIPC,
  deleteConversationIPC,
} from '../lib/tauri';

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
  status: 'running' | 'complete';
  /** Node IDs visited by this step (Feature 4: Live Visualization) */
  nodeIds?: string[];
}

/** Per-provider cost rates ($ per 1M tokens, blended input+output) */
export const PROVIDER_COST_RATES: Record<string, number> = {
  groq: 0.10,
  google: 0.0,
  openrouter: 1.50,
  agentrouter: 0.75,
  ollama: 0.0,
};

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

  createConversation: (title: string, docId?: string) => string;
  setActiveConversation: (id: string | null) => void;
  addMessage: (message: ChatMessage) => void;
  addExplorationStep: (step: ExplorationStep) => void;
  updateStepStatus: (stepNumber: number, status: ExplorationStep['status'], outputSummary?: string, nodeIds?: string[]) => void;
  setIsExploring: (exploring: boolean) => void;
  clearSteps: () => void;
  loadConversations: () => Promise<void>;
  loadMessages: (convId: string) => Promise<void>;
  deleteConversation: (convId: string) => Promise<void>;
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
    });
    // If selecting an existing conversation, load its messages
    if (id) {
      get().loadMessages(id);
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

  updateStepStatus: (stepNumber: number, status: ExplorationStep['status'], outputSummary?: string, nodeIds?: string[]) => {
    set((state) => {
      const newVisited = nodeIds
        ? [...state.visitedNodeIds, ...nodeIds.filter((id) => !state.visitedNodeIds.includes(id))]
        : state.visitedNodeIds;

      return {
        explorationSteps: state.explorationSteps.map((step) =>
          step.stepNumber === stepNumber
            ? { ...step, status, ...(outputSummary !== undefined ? { outputSummary } : {}), ...(nodeIds ? { nodeIds } : {}) }
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
    set({ explorationSteps: [], visitedNodeIds: [], activeNodeId: null });
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
          ? { activeConversationId: null, messages: [], explorationSteps: [] }
          : {}),
      }));
    } catch (err) {
      console.warn('Failed to delete conversation:', err);
    }
  },
}));
