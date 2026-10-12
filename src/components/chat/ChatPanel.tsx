import { useState, useRef, useEffect, useCallback, Fragment } from 'react';
import { Send, Square, FileText, AlertCircle, Upload, Plus, LoaderCircle } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeSanitize from 'rehype-sanitize';
import clsx from 'clsx';
import { useChatStore, type ExplorationStep, type ChatMessage } from '../../stores/chat';
import { useDocumentsStore } from '../../stores/documents';
import { useSettingsStore } from '../../stores/settings';
import { chatWithAgent, abortQuery, type ChatEvent } from '../../lib/tauri';
import { ProcessSummary } from './ProcessSummary';
import { ChatEmptyState } from './ChatEmptyState';
import { ModelPicker } from './ModelPicker';
import { useAddDocument } from '../../hooks/useAddDocument';
import { useLocalModelStore } from '../../stores/localModel';
import { isLocalProvider } from '../../lib/providers';
import { useLocalServersStore, firstUsableServer } from '../../stores/localServers';
import { MessageActions } from './MessageActions';
import { DictationButton } from './DictationButton';
import styles from './ChatPanel.module.css';

interface ChatPanelProps {
  onOpenSettings?: () => void;
  onOpenDocs?: () => void;
  /** Switch answering to the offline model (and offer its download). */
  onSetupOffline?: () => void;
}

export function ChatPanel({ onOpenSettings, onOpenDocs, onSetupOffline }: ChatPanelProps) {
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
  } = useChatStore();

  const { documents } = useDocumentsStore();
  const { pickAndAdd, addFromPath, isIngesting } = useAddDocument();
  const { providers, activeProviderId } = useSettingsStore();

  const activeProvider = providers.find((p) => p.id === activeProviderId);
  const localModelDownloaded = useLocalModelStore((s) => s.status?.downloaded ?? false);
  const localModelMissing = isLocalProvider(activeProvider) && !localModelDownloaded;
  const localServer = firstUsableServer(useLocalServersStore((s) => s.servers));
  const selectServerModel = useSettingsStore((s) => s.selectServerModel);

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

  /** Send the composer text, or `text` when given (starter questions). */
  const handleSend = async (text?: string) => {
    const trimmed = (text ?? input).trim();
    if (!trimmed || isExploring) return;

    const docIds = [...conversationDocIds];

    if (!activeProviderId || !activeProvider) {
      setSendError('Choose how to answer first: the offline model or a cloud provider.');
      return;
    }

    if (localModelMissing) {
      setSendError('Download the offline model first (Settings → Local AI).');
      return;
    }

    if (docIds.length === 0) {
      setSendError('Add a document to this chat first — answers come from your documents.');
      return;
    }

    setSendError(null);

    const convId = activeConversationId ?? createConversation(trimmed.slice(0, 40));

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
    for (const file of Array.from(e.dataTransfer.files)) {
      const filePath = (file as unknown as { path?: string }).path ?? file.name;
      if (filePath) await addFromPath(filePath);
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
            <ChatEmptyState
              hasProvider={!noProvider && !localModelMissing}
              localModelMissing={localModelMissing}
              localServer={localServer && { label: localServer.label, model: localServer.models[0] }}
              onUseLocalServer={
                localServer
                  ? () => void selectServerModel(localServer.provider, localServer.models[0], localServer.base_url)
                  : undefined
              }
              onUseOffline={() => onSetupOffline?.()}
              docNames={convDocNames as string[]}
              isIngesting={isIngesting}
              onOpenSettings={() => onOpenSettings?.()}
              onAddDocument={() => void pickAndAdd()}
              onFocusInput={() => textareaRef.current?.focus()}
              onAsk={(q) => void handleSend(q)}
            />
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
                      <div className={clsx(styles.assistantRow, 'answer-row')}>
                        <div className={styles.assistantContent}>
                          <ReactMarkdown remarkPlugins={[remarkGfm]} rehypePlugins={[rehypeSanitize]}>
                            {msg.content}
                          </ReactMarkdown>
                          <MessageActions content={msg.content} />
                        </div>
                      </div>
                    )}

                    {i === lastUserIdx && (
                      <>
                        <ProcessSummary steps={explorationSteps} active={isExploring} />

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
            {convDocNames.length > 0 ? (
              <button type="button" className={styles.docChip} onClick={onOpenDocs} title="Documents in this chat">
                <FileText size={12} />
                <span>
                  {convDocNames.length === 1
                    ? convDocNames[0]
                    : `${convDocNames.length} documents`}
                </span>
              </button>
            ) : null}
            <button
              type="button"
              className={styles.docChip}
              onClick={() => void pickAndAdd()}
              disabled={isIngesting}
              title="Add a document to this chat"
            >
              {isIngesting ? <LoaderCircle size={12} className={styles.spin} /> : <Plus size={12} />}
              <span>{isIngesting ? 'Adding…' : convDocNames.length > 0 ? 'Add' : 'Add document'}</span>
            </button>
            <ModelPicker onOpenSettings={() => onOpenSettings?.()} />
          </div>

          <div className={styles.inputRow}>
            <textarea
              id="chat-input"
              ref={textareaRef}
              className={styles.textarea}
              placeholder={
                noProvider
                  ? 'Choose offline or cloud answers to start…'
                  : localModelMissing
                    ? 'Download the offline model to start…'
                    : noDocs
                    ? 'Add a document, then ask anything about it…'
                    : 'Ask about your documents…'
              }
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              rows={1}
              aria-label="Message input"
              disabled={isExploring}
            />
            <DictationButton
              onTranscript={(text) => {
                setInput((prev) => (prev.trim() ? `${prev.trimEnd()} ${text}` : text));
                textareaRef.current?.focus();
              }}
              disabled={isExploring}
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
                onClick={() => void handleSend()}
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
