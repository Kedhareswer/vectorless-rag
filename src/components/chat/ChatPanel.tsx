import { useState, useRef, useEffect, useCallback, Fragment } from 'react';
import { Send, Square, FileText, Compass, AlertCircle, Server, Upload, FolderOpen, Settings } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeSanitize from 'rehype-sanitize';
import clsx from 'clsx';
import { useChatStore, type ExplorationStep, type ChatMessage } from '../../stores/chat';
import { useDocumentsStore } from '../../stores/documents';
import { useSettingsStore } from '../../stores/settings';
import { chatWithAgent, abortQuery, type ChatEvent } from '../../lib/tauri';
import { ThinkingBlock } from './ThinkingBlock';
import styles from './ChatPanel.module.css';

interface ChatPanelProps {
  onOpenSettings?: () => void;
  onOpenDocs?: () => void;
}

export function ChatPanel({ onOpenSettings, onOpenDocs }: ChatPanelProps) {
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
    conversationDocIds,
    addMessage,
    createConversation,
    addExplorationStep,
    updateStepStatus,
    setIsExploring,
    clearSteps,
    addDocToActiveConversation,
  } = useChatStore();

  const { documents, ingestDocumentFromPath } = useDocumentsStore();
  const { providers, activeProviderId } = useSettingsStore();

  const activeProvider = providers.find((p) => p.id === activeProviderId);

  const streamingContentRef = useRef('');
  const [streamingContent, setStreamingContent] = useState<string | null>(null);

  const handleChatEvent = useCallback((event: ChatEvent) => {
    switch (event.type) {
      case 'step-start': {
        const step: ExplorationStep = {
          stepNumber: event.stepNumber,
          tool: event.tool,
          inputSummary: event.inputSummary,
          outputSummary: '',
          tokensUsed: 0,
          latencyMs: 0,
          cost: 0,
          status: 'running',
        };
        addExplorationStep(step);
        break;
      }
      case 'step-complete':
        updateStepStatus(event.stepNumber, 'complete', event.outputSummary, event.nodeIds, event.tokensUsed, event.latencyMs, event.cost);
        break;
      case 'token':
        if (!event.done) {
          streamingContentRef.current += event.token;
          setStreamingContent(streamingContentRef.current);
        }
        break;
      case 'response': {
        const msg: ChatMessage = {
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
          role: 'assistant',
          content: event.content,
          createdAt: new Date().toISOString(),
        };
        addMessage(msg);
        setIsExploring(false);
        streamingContentRef.current = '';
        setStreamingContent(null);
        break;
      }
      case 'error':
        setSendError(event.error);
        setIsExploring(false);
        streamingContentRef.current = '';
        setStreamingContent(null);
        break;
    }
  }, [addExplorationStep, updateStepStatus, addMessage, setIsExploring]);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, explorationSteps, streamingContent]);

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

    const docIds = [...conversationDocIds];

    if (docIds.length === 0) {
      setSendError('No documents attached to this chat. Add a document first.');
      return;
    }

    if (!activeProviderId || !activeProvider) {
      setSendError('Please configure an LLM provider in Settings.');
      return;
    }

    setSendError(null);

    let convId = activeConversationId;
    if (!convId) {
      convId = createConversation(trimmed.slice(0, 40));
      for (const docId of docIds) {
        await addDocToActiveConversation(docId);
      }
    }

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

    setIsExploring(true);
    clearSteps();

    try {
      await chatWithAgent(trimmed, docIds, activeProviderId, handleChatEvent, convId ?? undefined);
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
        const summary = await ingestDocumentFromPath(filePath);
        if (summary) {
          await addDocToActiveConversation(summary.id);
        }
      }
    }
  };

  const convDocNames = conversationDocIds
    .map((id) => documents.find((d) => d.id === id)?.name)
    .filter(Boolean);

  const hasContent = messages.length > 0 || explorationSteps.length > 0;
  const noProvider = providers.length === 0;
  const noDocs = conversationDocIds.length === 0;

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
          <span>Drop file to add to this chat</span>
        </div>
      )}

      {/* Messages area — centered column */}
      <div className={styles.messages}>
        <div className={styles.messagesInner}>
          {!hasContent ? (
            <div className={styles.empty}>
              <div className={styles.emptyIconWrap}>
                <Compass size={32} />
              </div>
              <h3 className={styles.emptyTitle}>
                Start exploring
              </h3>
              <p className={styles.emptySubtitle}>
                {!activeConversationId
                  ? 'Create a new chat from the top bar, then add documents to explore'
                  : noDocs
                    ? 'Add documents and ask questions \u2014 I\'ll navigate the structure to find answers'
                    : 'Add documents and ask questions \u2014 I\'ll navigate the structure to find answers'}
              </p>
              <div className={styles.emptyActions}>
                {noProvider && onOpenSettings && (
                  <button type="button" className={styles.emptyAction} onClick={onOpenSettings}>
                    <Settings size={14} />
                    <span>Configure Provider</span>
                  </button>
                )}
                {noDocs && activeConversationId && onOpenDocs && (
                  <button type="button" className={styles.emptyAction} onClick={onOpenDocs}>
                    <FolderOpen size={14} />
                    <span>Add Documents</span>
                  </button>
                )}
              </div>
            </div>
          ) : (
            <>
              {(() => {
                const lastUserIdx = messages.reduce(
                  (acc, msg, i) => (msg.role === 'user' ? i : acc),
                  -1,
                );

                return messages.map((msg, i) => (
                  <Fragment key={msg.id}>
                    {msg.role === 'user' ? (
                      <div className={styles.userRow}>
                        <div className={styles.userBubble}>
                          {msg.content}
                        </div>
                      </div>
                    ) : (
                      <div className={styles.assistantRow}>
                        <div className={styles.assistantContent}>
                          <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSanitize]}>
                            {msg.content}
                          </ReactMarkdown>
                        </div>
                      </div>
                    )}

                    {i === lastUserIdx && (
                      <>
                        {explorationSteps.length > 0 && (
                          <div className={styles.stepsGroup}>
                            {explorationSteps.map((step) => (
                              <ThinkingBlock key={step.stepNumber} step={step} />
                            ))}
                          </div>
                        )}

                        {isExploring && explorationSteps.length === 0 && !streamingContent && (
                          <div className={styles.exploringIndicator}>
                            <span className={styles.exploringDot} />
                            <span>Analyzing document...</span>
                          </div>
                        )}

                        {streamingContent && (
                          <div className={styles.assistantRow}>
                            <div className={clsx(styles.assistantContent, styles.streaming)}>
                              <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSanitize]}>
                                {streamingContent}
                              </ReactMarkdown>
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
      </div>

      {/* Floating input bar */}
      <div className={styles.inputBarWrap}>
        <div className={styles.inputBar}>
          <div className={styles.inputMeta}>
            {convDocNames.length > 0 && (
              <button type="button" className={styles.docChip} onClick={onOpenDocs}>
                <FileText size={12} />
                <span>
                  {convDocNames.length === 1
                    ? convDocNames[0]
                    : `${convDocNames.length} documents`}
                </span>
              </button>
            )}
            {activeProvider && (
              <div className={styles.providerChip}>
                <Server size={10} />
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
                  : !activeConversationId
                    ? 'Create a new chat to get started...'
                    : noDocs
                      ? 'Add a document to this chat first...'
                      : 'Ask about your documents...'
              }
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              rows={1}
              aria-label="Message input"
              disabled={isExploring || !activeConversationId}
            />
            {isExploring ? (
              <button
                type="button"
                className={clsx(styles.sendBtn, styles.stopBtn)}
                onClick={() => abortQuery().catch(() => {})}
                title="Stop query"
              >
                <Square size={14} fill="currentColor" />
              </button>
            ) : (
              <button
                type="button"
                className={clsx(styles.sendBtn, input.trim() && styles.sendBtnActive)}
                onClick={handleSend}
                disabled={!input.trim()}
                title="Send message (Enter)"
              >
                <Send size={16} />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
