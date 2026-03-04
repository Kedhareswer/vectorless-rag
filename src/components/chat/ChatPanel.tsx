import { useState, useRef, useEffect } from 'react';
import { Send, FileText, Compass, AlertCircle, Server } from 'lucide-react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import clsx from 'clsx';
import { useChatStore, type ExplorationStep, type ChatMessage } from '../../stores/chat';
import { useDocumentsStore } from '../../stores/documents';
import { useSettingsStore } from '../../stores/settings';
import { chatWithAgent } from '../../lib/tauri';
import { ThinkingBlock } from './ThinkingBlock';
import styles from './ChatPanel.module.css';

interface ExplorationStepPayload {
  step_number: number;
  tool: string;
  input_summary: string;
}

interface ExplorationStepCompletePayload {
  step_number: number;
  output_summary: string;
  tokens_used: number;
  latency_ms: number;
}

interface ChatResponsePayload {
  content: string;
}

interface ChatErrorPayload {
  error: string;
}

export function ChatPanel() {
  const [input, setInput] = useState('');
  const [sendError, setSendError] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const {
    messages,
    explorationSteps,
    isExploring,
    activeConversationId,
    addMessage,
    createConversation,
    addExplorationStep,
    updateStepStatus,
    setIsExploring,
    clearSteps,
  } = useChatStore();

  const { documents, activeDocumentId, setActiveDocument } = useDocumentsStore();
  const { providers, activeProviderId } = useSettingsStore();

  const activeProvider = providers.find((p) => p.id === activeProviderId);

  // Listen for Tauri events
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];

    const setupListeners = async () => {
      try {
        const unlistenStepStart = await listen<ExplorationStepPayload>(
          'exploration-step-start',
          (event) => {
            const payload = event.payload;
            const step: ExplorationStep = {
              stepNumber: payload.step_number,
              tool: payload.tool,
              inputSummary: payload.input_summary,
              outputSummary: '',
              tokensUsed: 0,
              latencyMs: 0,
              status: 'running',
            };
            addExplorationStep(step);
          }
        );
        unlisteners.push(unlistenStepStart);

        const unlistenStepComplete = await listen<ExplorationStepCompletePayload>(
          'exploration-step-complete',
          (event) => {
            const payload = event.payload;
            updateStepStatus(payload.step_number, 'complete', payload.output_summary);
          }
        );
        unlisteners.push(unlistenStepComplete);

        const unlistenResponse = await listen<ChatResponsePayload>(
          'chat-response',
          (event) => {
            const msg: ChatMessage = {
              id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
              role: 'assistant',
              content: event.payload.content,
              createdAt: new Date().toISOString(),
            };
            addMessage(msg);
            setIsExploring(false);
          }
        );
        unlisteners.push(unlistenResponse);

        const unlistenError = await listen<ChatErrorPayload>(
          'chat-error',
          (event) => {
            setSendError(event.payload.error);
            setIsExploring(false);
          }
        );
        unlisteners.push(unlistenError);
      } catch (err) {
        // Tauri not available (running in browser) -- listeners fail silently
        console.warn('Tauri event listeners not available:', err);
      }
    };

    setupListeners();

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [addExplorationStep, updateStepStatus, addMessage, setIsExploring]);

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, explorationSteps]);

  // Auto-grow textarea
  useEffect(() => {
    const textarea = textareaRef.current;
    if (textarea) {
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.min(textarea.scrollHeight, 160)}px`;
    }
  }, [input]);

  const handleSend = async () => {
    const trimmed = input.trim();
    if (!trimmed) return;

    // Validate prerequisites
    if (!activeDocumentId) {
      setSendError('Please select a document first.');
      return;
    }

    if (!activeProviderId || !activeProvider) {
      setSendError('Please configure an LLM provider in Settings.');
      return;
    }

    setSendError(null);

    // Create conversation if none exists
    let convId = activeConversationId;
    if (!convId) {
      convId = createConversation(trimmed.slice(0, 40), activeDocumentId ?? undefined);
    }

    // Add user message to store
    addMessage({
      id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
      role: 'user',
      content: trimmed,
      createdAt: new Date().toISOString(),
    });

    setInput('');
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }

    // Begin exploration
    setIsExploring(true);
    clearSteps();

    try {
      await chatWithAgent(trimmed, activeDocumentId, activeProviderId);
    } catch (err) {
      setSendError(String(err));
      setIsExploring(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const hasContent = messages.length > 0 || explorationSteps.length > 0;
  const noProvider = providers.length === 0;
  const noDocument = documents.length === 0;

  return (
    <div className={styles.panel}>
      {/* Messages area */}
      <div className={styles.messages}>
        {!hasContent ? (
          <div className={styles.empty}>
            <Compass size={40} className={styles.emptyIcon} />
            <h3 className={styles.emptyTitle}>Drop a document and start exploring</h3>
            <p className={styles.emptySubtitle}>
              Add a document, then ask questions to navigate its contents with AI
            </p>
            {noProvider && (
              <div className={styles.emptyWarning}>
                <AlertCircle size={14} />
                <span>No LLM provider configured. Open Settings to add one.</span>
              </div>
            )}
            {noDocument && !noProvider && (
              <div className={styles.emptyWarning}>
                <AlertCircle size={14} />
                <span>No documents loaded. Add a document from the sidebar.</span>
              </div>
            )}
          </div>
        ) : (
          <>
            {messages.map((msg) => (
              <div
                key={msg.id}
                className={clsx(
                  styles.message,
                  msg.role === 'user' ? styles.messageUser : styles.messageAssistant
                )}
              >
                <div
                  className={clsx(
                    styles.bubble,
                    msg.role === 'user' ? styles.bubbleUser : styles.bubbleAssistant
                  )}
                >
                  {msg.content}
                </div>
              </div>
            ))}

            {explorationSteps.map((step) => (
              <div key={step.stepNumber} className={styles.stepWrapper}>
                <ThinkingBlock step={step} />
              </div>
            ))}

            {isExploring && explorationSteps.length === 0 && (
              <div className={styles.stepWrapper}>
                <div className={styles.exploringIndicator}>
                  <span className={styles.exploringDot} />
                  <span>Exploring document...</span>
                </div>
              </div>
            )}
          </>
        )}

        {sendError && (
          <div className={styles.errorMessage}>
            <AlertCircle size={14} />
            <span>{sendError}</span>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input bar */}
      <div className={styles.inputBar}>
        <div className={styles.inputMeta}>
          {documents.length > 0 && (
            <div className={styles.docSelector}>
              <FileText size={14} className={styles.docSelectorIcon} />
              <select
                className={styles.docSelect}
                value={activeDocumentId ?? ''}
                onChange={(e) => setActiveDocument(e.target.value || null)}
              >
                <option value="">No document</option>
                {documents.map((doc) => (
                  <option key={doc.id} value={doc.id}>
                    {doc.name}
                  </option>
                ))}
              </select>
            </div>
          )}

          {activeProvider && (
            <div className={styles.providerBadge}>
              <Server size={12} />
              <span>{activeProvider.name}/{activeProvider.model}</span>
            </div>
          )}
        </div>

        <div className={styles.inputRow}>
          <textarea
            ref={textareaRef}
            className={styles.textarea}
            placeholder={
              noProvider
                ? 'Configure a provider in Settings first...'
                : noDocument
                  ? 'Add a document to get started...'
                  : 'Ask about your document...'
            }
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={1}
            disabled={isExploring}
          />
          <button
            className={clsx(styles.sendButton, input.trim() && styles.sendButtonActive)}
            onClick={handleSend}
            disabled={!input.trim() || isExploring}
            title="Send message"
          >
            <Send size={18} />
          </button>
        </div>
      </div>
    </div>
  );
}
