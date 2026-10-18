import { useEffect, useState, useCallback, lazy, Suspense } from 'react';
import { useThemeStore } from './stores/theme';
import { useChatStore } from './stores/chat';
import { useDocumentsStore } from './stores/documents';
import { useSettingsStore } from './stores/settings';
import { TopBar } from './components/common/TopBar';
import { ConversationSwitcher } from './components/common/ConversationSwitcher';
import { SlidePanel } from './components/common/SlidePanel';
import { ChatPanel } from './components/chat/ChatPanel';
import { DocsPanel } from './components/preview/DocsPanel';
import { TracePanel } from './components/preview/TracePanel';
import { BatchPanel } from './components/preview/BatchPanel';
import { SettingsModal } from './components/settings/SettingsModal';
import { ModelDownloadDialog } from './components/common/ModelDownloadDialog';
import { useGuidesStore } from './stores/guides';
import { getSetting, setSetting, checkLocalModel } from './lib/tauri';
import { useLocalModelStore } from './stores/localModel';
import { useLocalServersStore } from './stores/localServers';
import { useShortcuts } from './hooks/useShortcuts';
import styles from './App.module.css';

// Shown once or on demand, so kept out of the startup bundle.
const WelcomeTour = lazy(() => import('./components/onboarding/WelcomeTour').then((m) => ({ default: m.WelcomeTour })));
const AddModelDialog = lazy(() => import('./components/onboarding/AddModelDialog').then((m) => ({ default: m.AddModelDialog })));

function App() {
  const initialize = useThemeStore((s) => s.initialize);

  // Panel states
  const [showSwitcher, setShowSwitcher] = useState(false);
  const [showDocsPanel, setShowDocsPanel] = useState(false);
  const [showTracePanel, setShowTracePanel] = useState(false);
  const [showBatchPanel, setShowBatchPanel] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showModelDialog, setShowModelDialog] = useState(false);

  // Stores
  const activeConversationId = useChatStore((s) => s.activeConversationId);
  const conversations = useChatStore((s) => s.conversations);
  const loadConversations = useChatStore((s) => s.loadConversations);
  const resumeLastConversation = useChatStore((s) => s.resumeLastConversation);
  const startNewChat = useChatStore((s) => s.startNewChat);
  const docsCount = useChatStore((s) => s.conversationDocIds.length);
  const loadDocuments = useDocumentsStore((s) => s.loadDocuments);
  const loadProviders = useSettingsStore((s) => s.loadProviders);
  const enableLocalProvider = useSettingsStore((s) => s.enableLocalProvider);
  const refreshLocalModel = useLocalModelStore((s) => s.refreshStatus);
  const detectLocalServers = useLocalServersStore((s) => s.detect);
  const { tourOpen, openTour, closeTour, addModelOpen, addModelPath, closeAddModel } = useGuidesStore();

  const activeConv = conversations.find((c) => c.id === activeConversationId);

  useEffect(() => {
    initialize();
    // Reopen where the user left off instead of an empty screen.
    loadConversations().then(resumeLastConversation);
    loadDocuments();
    loadProviders();
    refreshLocalModel();
    // Models the user already has (Ollama / LM Studio) can answer without a download.
    detectLocalServers();
  }, [refreshLocalModel, detectLocalServers, initialize, loadConversations, resumeLastConversation, loadDocuments, loadProviders]);

  // First run: a short welcome tour. Setting up a model is shown later, when one is needed.
  useEffect(() => {
    getSetting('onboarding_done')
      .then((done) => {
        if (!done) openTour();
      })
      .catch(() => {});
  }, [openTour]);

  const handleTourClose = () => {
    closeTour();
    setSetting('onboarding_done', '1').catch(() => {});
  };

  const toggleDocsPanel = useCallback(() => {
    setShowDocsPanel((v) => !v);
  }, []);

  const toggleBatchPanel = useCallback(() => {
    setShowBatchPanel((v) => !v);
  }, []);

  const toggleTracePanel = useCallback(() => {
    setShowTracePanel((v) => !v);
  }, []);

  /** Answer offline: select the Local provider, then offer the model download if needed. */
  const setupOffline = useCallback(async () => {
    await enableLocalProvider();
    const status = await checkLocalModel().catch(() => null);
    if (!status?.downloaded) setShowModelDialog(true);
  }, [enableLocalProvider]);

  const focusInput = useCallback(() => {
    // Let any panel/dropdown close first so focus lands in the composer.
    requestAnimationFrame(() => document.getElementById('chat-input')?.focus());
  }, []);

  const handleNewChat = useCallback(() => {
    startNewChat();
    setShowSwitcher(false);
    focusInput();
  }, [startNewChat, focusInput]);

  useShortcuts({
    newChat: handleNewChat,
    switchChat: () => setShowSwitcher((v) => !v),
    documents: toggleDocsPanel,
    trace: toggleTracePanel,
    batch: toggleBatchPanel,
    settings: () => setShowSettings(true),
    focusInput,
  });

  return (
    <div className={styles.layout}>
      <TopBar
        chatTitle={activeConv?.title ?? null}
        switcherOpen={showSwitcher}
        onSwitcherToggle={() => setShowSwitcher((v) => !v)}
        onNewChat={handleNewChat}
        docsCount={docsCount}
        docsPanelOpen={showDocsPanel}
        onDocsToggle={toggleDocsPanel}
        tracePanelOpen={showTracePanel}
        onTraceToggle={toggleTracePanel}
        batchPanelOpen={showBatchPanel}
        onBatchToggle={toggleBatchPanel}
        onSettingsClick={() => setShowSettings(true)}
      />

      <main className={styles.main}>
        <ChatPanel
          onOpenSettings={() => setShowSettings(true)}
          onOpenDocs={toggleDocsPanel}
          onSetupOffline={() => void setupOffline()}
        />
      </main>

      {/* Conversation switcher dropdown */}
      {showSwitcher && (
        <ConversationSwitcher onClose={() => setShowSwitcher(false)} />
      )}

      {/* Document slide-over panel */}
      {showDocsPanel && (
        <SlidePanel title="Documents" onClose={() => setShowDocsPanel(false)}>
          <DocsPanel />
        </SlidePanel>
      )}

      {/* Batch questions slide-over panel */}
      {showBatchPanel && (
        <SlidePanel title="Batch Questions" onClose={() => setShowBatchPanel(false)}>
          <BatchPanel />
        </SlidePanel>
      )}

      {/* Trace slide-over panel */}
      {showTracePanel && (
        <SlidePanel title="Trace & Evaluation" onClose={() => setShowTracePanel(false)}>
          <TracePanel />
        </SlidePanel>
      )}

      {/* Settings modal */}
      {showSettings && <SettingsModal onClose={() => setShowSettings(false)} />}

      {showModelDialog && (
        <ModelDownloadDialog
          onClose={() => {
            setShowModelDialog(false);
            void refreshLocalModel();
          }}
        />
      )}

      <Suspense fallback={null}>
        {tourOpen && <WelcomeTour onClose={handleTourClose} />}

        {/* How to add a model, opened when the user first needs one */}
        {addModelOpen && (
          <AddModelDialog
            initialPath={addModelPath}
            onClose={closeAddModel}
            onSetupOffline={() => void setupOffline()}
          />
        )}
      </Suspense>
    </div>
  );
}

export default App;
