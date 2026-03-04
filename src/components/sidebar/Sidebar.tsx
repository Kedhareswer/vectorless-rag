import { useState, useEffect } from 'react';
import {
  FileText,
  MessageSquare,
  Settings,
  Plus,
  PanelLeftClose,
  PanelLeftOpen,
  Loader,
  Trash2,
} from 'lucide-react';
import clsx from 'clsx';
import { IconButton } from '../common/IconButton';
import { SettingsModal } from '../settings/SettingsModal';
import { useDocumentsStore } from '../../stores/documents';
import { useChatStore } from '../../stores/chat';
import { useSettingsStore } from '../../stores/settings';
import { openFileDialog } from '../../lib/tauri';
import styles from './Sidebar.module.css';

type SidebarTab = 'documents' | 'chats' | 'settings';

export function Sidebar() {
  const [collapsed, setCollapsed] = useState(false);
  const [activeTab, setActiveTab] = useState<SidebarTab>('documents');
  const [showSettings, setShowSettings] = useState(false);

  const {
    documents,
    activeDocumentId,
    isIngesting,
    error: docError,
    setActiveDocument,
    loadDocuments,
    ingestDocumentFromPath,
    deleteDocumentFromBackend,
    loadActiveTree,
  } = useDocumentsStore();

  const { conversations, activeConversationId, setActiveConversation, createConversation } =
    useChatStore();

  const { providers, loadProviders } = useSettingsStore();

  // Load documents and providers on mount
  useEffect(() => {
    loadDocuments();
    loadProviders();
  }, [loadDocuments, loadProviders]);

  // Load active tree when document selection changes
  useEffect(() => {
    if (activeDocumentId) {
      loadActiveTree(activeDocumentId);
    }
  }, [activeDocumentId, loadActiveTree]);

  const tabs: { id: SidebarTab; icon: typeof FileText; label: string }[] = [
    { id: 'documents', icon: FileText, label: 'Documents' },
    { id: 'chats', icon: MessageSquare, label: 'Chats' },
    { id: 'settings', icon: Settings, label: 'Settings' },
  ];

  const handleTabClick = (tabId: SidebarTab) => {
    if (tabId === 'settings') {
      setShowSettings(true);
    } else {
      setActiveTab(tabId);
    }
  };

  const handleNewChat = () => {
    createConversation('New Chat');
  };

  const handleAddDocument = async () => {
    try {
      const filePath = await openFileDialog();
      if (filePath) {
        await ingestDocumentFromPath(filePath);
      }
    } catch (err) {
      console.warn('Failed to add document:', err);
    }
  };

  const handleDeleteDocument = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    await deleteDocumentFromBackend(id);
  };

  const activeProviderCount = providers.filter((p) => p.isActive).length;

  return (
    <>
      <aside className={clsx(styles.sidebar, collapsed && styles.collapsed)}>
        <div className={styles.header}>
          <IconButton
            icon={collapsed ? PanelLeftOpen : PanelLeftClose}
            onClick={() => setCollapsed(!collapsed)}
            title={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            size="sm"
          />
        </div>

        <nav className={styles.tabs}>
          {tabs.map((tab) => (
            <button
              key={tab.id}
              className={clsx(
                styles.tab,
                activeTab === tab.id && tab.id !== 'settings' && styles.tabActive
              )}
              onClick={() => handleTabClick(tab.id)}
              title={tab.label}
            >
              <tab.icon size={18} />
              {!collapsed && (
                <span className={styles.tabLabel}>
                  {tab.label}
                  {tab.id === 'settings' && activeProviderCount > 0 && (
                    <span className={styles.badge}>{activeProviderCount}</span>
                  )}
                </span>
              )}
            </button>
          ))}
        </nav>

        {!collapsed && (
          <div className={styles.content}>
            {activeTab === 'documents' && (
              <div className={styles.section}>
                <div className={styles.sectionHeader}>
                  <span className={styles.sectionTitle}>Documents</span>
                </div>

                {docError && (
                  <p className={styles.errorText}>{docError}</p>
                )}

                <div className={styles.list}>
                  {documents.length === 0 ? (
                    <p className={styles.emptyText}>No documents yet</p>
                  ) : (
                    documents.map((doc) => (
                      <button
                        key={doc.id}
                        className={clsx(
                          styles.listItem,
                          activeDocumentId === doc.id && styles.listItemActive
                        )}
                        onClick={() => setActiveDocument(doc.id)}
                      >
                        <FileText size={14} className={styles.listItemIcon} />
                        <span className={styles.listItemName}>{doc.name}</span>
                        <span className={styles.badge}>{doc.docType}</span>
                        <button
                          className={styles.listItemDelete}
                          onClick={(e) => handleDeleteDocument(e, doc.id)}
                          title="Delete document"
                          type="button"
                        >
                          <Trash2 size={12} />
                        </button>
                      </button>
                    ))
                  )}
                </div>

                <button
                  className={styles.addButton}
                  onClick={handleAddDocument}
                  disabled={isIngesting}
                >
                  {isIngesting ? (
                    <>
                      <Loader size={16} className={styles.spinner} />
                      <span>Ingesting...</span>
                    </>
                  ) : (
                    <>
                      <Plus size={16} />
                      <span>Add Document</span>
                    </>
                  )}
                </button>
              </div>
            )}

            {activeTab === 'chats' && (
              <div className={styles.section}>
                <div className={styles.sectionHeader}>
                  <span className={styles.sectionTitle}>Conversations</span>
                </div>
                <div className={styles.list}>
                  {conversations.length === 0 ? (
                    <p className={styles.emptyText}>No conversations yet</p>
                  ) : (
                    conversations.map((conv) => (
                      <button
                        key={conv.id}
                        className={clsx(
                          styles.listItem,
                          activeConversationId === conv.id && styles.listItemActive
                        )}
                        onClick={() => setActiveConversation(conv.id)}
                      >
                        <MessageSquare size={14} className={styles.listItemIcon} />
                        <span className={styles.listItemName}>{conv.title}</span>
                      </button>
                    ))
                  )}
                </div>
                <button className={styles.addButton} onClick={handleNewChat}>
                  <Plus size={16} />
                  <span>New Chat</span>
                </button>
              </div>
            )}
          </div>
        )}

        <div className={styles.about}>
          <span className={styles.aboutText}>Vectorless RAG v0.1.0</span>
        </div>
      </aside>

      {showSettings && <SettingsModal onClose={() => setShowSettings(false)} />}
    </>
  );
}
