import { useState, useRef, useEffect, Fragment } from 'react';
import { Send, Square, FileText, Compass, AlertCircle, Server, Upload } from 'lucide-react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeRaw from 'rehype-raw';
import clsx from 'clsx';
import { useChatStore, type ExplorationStep, type ChatMessage } from '../../stores/chat';
import { useDocumentsStore } from '../../stores/documents';
import { useSettingsStore } from '../../stores/settings';
import { chatWithAgent, abortQuery } from '../../lib/tauri';
import { ThinkingBlock } from './ThinkingBlock';
import styles from './ChatPanel.module.css';

interface ExplorationStepPayload {
  stepNumber: number;
  tool: string;
  inputSummary: string;
}

interface ExplorationStepCompletePayload {
  stepNumber: number;
  outputSummary: string;
  tokensUsed: number;
  latencyMs: number;
  cost: number;
  nodeIds: string[];
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
  const [isDragOver, setIsDragOver] = useState(false);
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

  const { documents, activeDocumentId, selectedDocumentIds, setActiveDocument, ingestDocumentFromPath } = useDocumentsStore();
  const { providers, activeProviderId } = useSettingsStore();

  const activeProvider = providers.find((p) => p.id === activeProviderId);

  // Listen for Tauri events
  // Store listener promises so cleanup can unlisten even if setup hasn't finished
  useEffect(() => {
    const listenerPromises: Promise<UnlistenFn>[] = [];

    try {
      listenerPromises.push(
        listen<ExplorationStepPayload>('exploration-step-start', (event) => {
          const payload = event.payload;
          const step: ExplorationStep = {
            stepNumber: payload.stepNumber,
            tool: payload.tool,
            inputSummary: payload.inputSummary,
            outputSummary: '',
            tokensUsed: 0,
            latencyMs: 0,
            cost: 0,
            status: 'running',
          };
          addExplorationStep(step);
        })
      );

      listenerPromises.push(
        listen<ExplorationStepCompletePayload>('exploration-step-complete', (event) => {
          const payload = event.payload;
          updateStepStatus(payload.stepNumber, 'complete', payload.outputSummary, payload.nodeIds, payload.tokensUsed, payload.latencyMs, payload.cost);
        })
      );

      listenerPromises.push(
        listen<ChatResponsePayload>('chat-response', (event) => {
          const msg: ChatMessage = {
            id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
            role: 'assistant',
            content: event.payload.content,
            createdAt: new Date().toISOString(),
          };
          addMessage(msg);
          setIsExploring(false);
        })
      );

      listenerPromises.push(
        listen<ChatErrorPayload>('chat-error', (event) => {
          setSendError(event.payload.error);
          setIsExploring(false);
        })
      );
    } catch (err) {
      console.warn('Tauri event listeners not available:', err);
    }

    return () => {
      // Each promise resolves to an unlisten function — call it when resolved
      listenerPromises.forEach((p) => p.then((unlisten) => unlisten()).catch(() => {}));
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

    // Build doc IDs array for multi-document queries
    const docIds = selectedDocumentIds.length > 0
      ? selectedDocumentIds
      : activeDocumentId ? [activeDocumentId] : [];

    // Validate prerequisites
    if (docIds.length === 0) {
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
      await chatWithAgent(trimmed, docIds, activeProviderId, convId ?? undefined);
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

  // Drag-and-drop handlers
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
  };

  const handleDrop = async (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const files = e.dataTransfer.files;
    if (files.length > 0) {
      const file = files[0];
      const filePath = (file as any).path || file.name;
      if (filePath) {
        await ingestDocumentFromPath(filePath);
      }
    }
  };

  const hasContent = messages.length > 0 || explorationSteps.length > 0;
  const noProvider = providers.length === 0;
  const noDocument = documents.length === 0;

  return (
    <div
      className={styles.panel}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {isDragOver && (
        <div className={styles.dropOverlay}>
          <Upload size={24} />
          <span>Drop file to start exploring</span>
        </div>
      )}
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
            {(() => {
              // Find last user message index to insert steps after it
              const lastUserIdx = messages.reduce(
                (acc, msg, i) => (msg.role === 'user' ? i : acc),
                -1,
              );

              return messages.map((msg, i) => (
                <Fragment key={msg.id}>
                  <div
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
                      {msg.role === 'assistant' ? (
                        <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeRaw]}>{msg.content}</ReactMarkdown>
                      ) : (
                        msg.content
                      )}
                    </div>
                  </div>

                  {/* Show exploration steps right after the last user message */}
                  {i === lastUserIdx && (
                    <>
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
                </Fragment>
              ));
            })()}
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
                aria-label="Select document"
                title="Select document"
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
          {isExploring ? (
            <button
              type="button"
              className={clsx(styles.sendButton, styles.stopButton)}
              onClick={() => abortQuery().catch(() => {})}
              title="Stop query"
            >
              <Square size={16} fill="currentColor" />
            </button>
          ) : (
            <button
              type="button"
              className={clsx(styles.sendButton, input.trim() && styles.sendButtonActive)}
              onClick={handleSend}
              disabled={!input.trim()}
              title="Send message"
            >
              <Send size={18} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
