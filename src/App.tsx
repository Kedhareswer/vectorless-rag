import { useEffect, useState, useCallback } from 'react';
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
import { SettingsModal } from './components/settings/SettingsModal';
import { ModelDownloadDialog } from './components/common/ModelDownloadDialog';
import { getSetting, setSetting, checkLocalModel } from './lib/tauri';
import { useShortcuts } from './hooks/useShortcuts';
import styles from './App.module.css';

function App() {
  const initialize = useThemeStore((s) => s.initialize);

  // Panel states
  const [showSwitcher, setShowSwitcher] = useState(false);
  const [showDocsPanel, setShowDocsPanel] = useState(false);
  const [showTracePanel, setShowTracePanel] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showOnboarding, setShowOnboarding] = useState(false);

  // Stores
  const activeConversationId = useChatStore((s) => s.activeConversationId);
  const conversations = useChatStore((s) => s.conversations);
  const loadConversations = useChatStore((s) => s.loadConversations);
  const resumeLastConversation = useChatStore((s) => s.resumeLastConversation);
  const startNewChat = useChatStore((s) => s.startNewChat);
  const docsCount = useChatStore((s) => s.conversationDocIds.length);
  const loadDocuments = useDocumentsStore((s) => s.loadDocuments);
  const loadProviders = useSettingsStore((s) => s.loadProviders);

  const activeConv = conversations.find((c) => c.id === activeConversationId);

  useEffect(() => {
    initialize();
    // Reopen where the user left off instead of an empty screen.
    loadConversations().then(resumeLastConversation);
    loadDocuments();
    loadProviders();
  }, [initialize, loadConversations, resumeLastConversation, loadDocuments, loadProviders]);

  // Onboarding check
  useEffect(() => {
    getSetting('onboarding_done').then((done) => {
      if (done) return;
      checkLocalModel().then((status) => {
        if (!status.downloaded) {
          setShowOnboarding(true);
        } else {
          setSetting('onboarding_done', '1').catch(() => {});
        }
      }).catch(() => {});
    }).catch(() => {});
  }, []);

  const handleOnboardingClose = () => {
    setShowOnboarding(false);
    setSetting('onboarding_done', '1').catch(() => {});
  };

  const toggleDocsPanel = useCallback(() => {
    setShowDocsPanel((v) => !v);
  }, []);

  const toggleTracePanel = useCallback(() => {
    setShowTracePanel((v) => !v);
  }, []);

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
        onSettingsClick={() => setShowSettings(true)}
      />

      <main className={styles.main}>
        <ChatPanel
          onOpenSettings={() => setShowSettings(true)}
          onOpenDocs={toggleDocsPanel}
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

      {/* Trace slide-over panel */}
      {showTracePanel && (
        <SlidePanel title="Trace & Evaluation" onClose={() => setShowTracePanel(false)}>
          <TracePanel />
        </SlidePanel>
      )}

      {/* Settings modal */}
      {showSettings && <SettingsModal onClose={() => setShowSettings(false)} />}

      {/* Onboarding model download */}
      {showOnboarding && (
        <ModelDownloadDialog onClose={handleOnboardingClose} isOnboarding />
      )}
    </div>
  );
}

export default App;
