import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useChatStore, titleFromMessage, NEW_CHAT_TITLE } from './chat';
import * as tauri from '../lib/tauri';

vi.mock('../lib/tauri', () => ({
  listConversations: vi.fn().mockResolvedValue([]),
  getConversationMessages: vi.fn().mockResolvedValue([]),
  saveConversationIPC: vi.fn().mockResolvedValue(undefined),
  saveMessageIPC: vi.fn().mockResolvedValue(undefined),
  deleteConversationIPC: vi.fn().mockResolvedValue(undefined),
  renameConversationIPC: vi.fn().mockResolvedValue(undefined),
  getTraces: vi.fn().mockResolvedValue([]),
  getSteps: vi.fn().mockResolvedValue([]),
  addDocToConversation: vi.fn().mockResolvedValue(undefined),
  getConversationDocIds: vi.fn().mockResolvedValue([]),
}));

vi.mock('./documents', () => ({
  useDocumentsStore: { getState: () => ({ setActiveDocument: vi.fn() }) },
}));

const reset = () =>
  useChatStore.setState({
    conversations: [],
    activeConversationId: null,
    messages: [],
    explorationSteps: [],
    conversationDocIds: [],
  });

const msg = (role: 'user' | 'assistant', content: string) => ({
  id: Math.random().toString(36), role, content, createdAt: new Date().toISOString(),
});

describe('chat navigation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    reset();
  });

  it('titleFromMessage keeps short questions and trims long ones at a word', () => {
    expect(titleFromMessage('  What   is the  notice period? ')).toBe('What is the notice period?');
    const long = titleFromMessage('How does the vendor contract renew and what are the exit options for us');
    expect(long.endsWith('…')).toBe(true);
    expect(long.length).toBeLessThanOrEqual(49);
    expect(long).not.toMatch(/\s…$/);
    expect(titleFromMessage('   ')).toBe(NEW_CHAT_TITLE);
  });

  it('names a new chat after its first question only', () => {
    const id = useChatStore.getState().createConversation(NEW_CHAT_TITLE);
    useChatStore.getState().addMessage(msg('user', 'What is the notice period?'));
    useChatStore.getState().addMessage(msg('assistant', '90 days.'));
    useChatStore.getState().addMessage(msg('user', 'And the fee?'));
    const conv = useChatStore.getState().conversations.find((c) => c.id === id)!;
    expect(conv.title).toBe('What is the notice period?');
    expect(tauri.renameConversationIPC).toHaveBeenCalledTimes(1);
  });

  it('does not rename chats that already have a title', () => {
    useChatStore.getState().createConversation('Contract review');
    useChatStore.getState().addMessage(msg('user', 'Anything'));
    expect(tauri.renameConversationIPC).not.toHaveBeenCalled();
  });

  it('startNewChat reuses an empty chat instead of piling up blanks', () => {
    const first = useChatStore.getState().startNewChat();
    const second = useChatStore.getState().startNewChat();
    expect(second).toBe(first);
    expect(useChatStore.getState().conversations).toHaveLength(1);

    useChatStore.getState().addMessage(msg('user', 'hello'));
    const third = useChatStore.getState().startNewChat();
    expect(third).not.toBe(first);
  });

  it('resumes the remembered chat, else the most recent', () => {
    const convs = [
      { id: 'a', title: 'A', docId: null, createdAt: '' },
      { id: 'b', title: 'B', docId: null, createdAt: '' },
    ];
    useChatStore.setState({ conversations: convs });
    useChatStore.getState().resumeLastConversation();
    expect(useChatStore.getState().activeConversationId).toBe('a');

    useChatStore.setState({ activeConversationId: null });
    localStorage.setItem('tgg-last-conversation', 'b');
    useChatStore.getState().resumeLastConversation();
    expect(useChatStore.getState().activeConversationId).toBe('b');
  });

  it('adding a document with no chat open creates one', async () => {
    await useChatStore.getState().addDocToActiveConversation('doc-1');
    const s = useChatStore.getState();
    expect(s.activeConversationId).not.toBeNull();
    expect(s.conversationDocIds).toEqual(['doc-1']);
    expect(tauri.addDocToConversation).toHaveBeenCalledWith(s.activeConversationId, 'doc-1');
  });
});
