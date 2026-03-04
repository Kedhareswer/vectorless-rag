import { useState, useEffect, useCallback } from 'react';
import {
  FileText,
  MessageSquare,
  Settings,
  Plus,
  PanelLeftClose,
  PanelLeftOpen,
  Loader,
  Trash2,
  Upload,
  CheckSquare,
  Square,
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
  const [isDragOver, setIsDragOver] = useState(false);

  const {
    documents,
    activeDocumentId,
    selectedDocumentIds,
    isIngesting,
    error: docError,
    setActiveDocument,
    toggleDocumentSelection,
    loadDocuments,
    ingestDocumentFromPath,
    deleteDocumentFromBackend,
    loadActiveTree,
  } = useDocumentsStore();

  const {
    conversations,
    activeConversationId,
    setActiveConversation,
    createConversation,
    loadConversations,
    deleteConversation,
  } = useChatStore();

  const { providers, loadProviders } = useSettingsStore();

  // Load documents, providers, and conversations on mount
  useEffect(() => {
    loadDocuments();
    loadProviders();
    loadConversations();
  }, [loadDocuments, loadProviders, loadConversations]);

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

  const handleDeleteConversation = async (e: React.MouseEvent, convId: string) => {
    e.stopPropagation();
    await deleteConversation(convId);
  };

  // Feature 2: Drag-and-drop handlers
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
  }, []);

  const handleDrop = useCallback(async (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);

    // In Tauri, files dragged from OS provide paths
    const files = e.dataTransfer.files;
    if (files.length > 0) {
      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        // Tauri webview gives us the path property on File objects
        const filePath = (file as unknown as { path?: string }).path ?? file.name;
        if (filePath) {
          try {
            await ingestDocumentFromPath(filePath);
          } catch (err) {
            console.warn('Failed to ingest dropped file:', err);
          }
        }
      }
    }
  }, [ingestDocumentFromPath]);

  // Feature 5: Multi-select with Ctrl/Cmd+click
  const handleDocClick = useCallback((e: React.MouseEvent, docId: string) => {
    if (e.ctrlKey || e.metaKey) {
      toggleDocumentSelection(docId);
    } else {
      setActiveDocument(docId);
    }
  }, [toggleDocumentSelection, setActiveDocument]);

  const activeProviderCount = providers.filter((p) => p.isActive).length;
  const multiSelectActive = selectedDocumentIds.length > 1;

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
              <div
                className={clsx(styles.section, isDragOver && styles.dropActive)}
                onDragOver={handleDragOver}
                onDragEnter={handleDragOver}
                onDragLeave={handleDragLeave}
                onDrop={handleDrop}
              >
                <div className={styles.sectionHeader}>
                  <span className={styles.sectionTitle}>Documents</span>
                  {multiSelectActive && (
                    <span className={styles.multiSelectBadge}>
                      {selectedDocumentIds.length} selected
                    </span>
                  )}
                </div>

                {docError && (
                  <p className={styles.errorText}>{docError}</p>
                )}

                {isDragOver && (
                  <div className={styles.dropOverlay}>
                    <Upload size={20} />
                    <span>Drop files to ingest</span>
                  </div>
                )}

                <div className={styles.list}>
                  {documents.length === 0 ? (
                    <p className={styles.emptyText}>No documents yet</p>
                  ) : (
                    documents.map((doc) => {
                      const isSelected = selectedDocumentIds.includes(doc.id);
                      return (
                        <div
                          key={doc.id}
                          tabIndex={0}
                          className={clsx(
                            styles.listItem,
                            activeDocumentId === doc.id && styles.listItemActive,
                            isSelected && multiSelectActive && styles.listItemSelected
                          )}
                          onClick={(e) => handleDocClick(e, doc.id)}
                          onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleDocClick(e as unknown as React.MouseEvent, doc.id); }}
                          title="Click to select. Ctrl+click for multi-select."
                        >
                          {multiSelectActive && (
                            isSelected
                              ? <CheckSquare size={14} className={styles.checkIcon} />
                              : <Square size={14} className={styles.listItemIcon} />
                          )}
                          {!multiSelectActive && <FileText size={14} className={styles.listItemIcon} />}
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
                        </div>
                      );
                    })
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
                      <div
                        key={conv.id}
                        tabIndex={0}
                        className={clsx(
                          styles.listItem,
                          activeConversationId === conv.id && styles.listItemActive
                        )}
                        onClick={() => setActiveConversation(conv.id)}
                        onKeyDown={(e) => { if (e.key === 'Enter' || e.key === ' ') setActiveConversation(conv.id); }}
                      >
                        <MessageSquare size={14} className={styles.listItemIcon} />
                        <span className={styles.listItemName}>{conv.title}</span>
                        <button
                          className={styles.listItemDelete}
                          onClick={(e) => handleDeleteConversation(e, conv.id)}
                          title="Delete conversation"
                          type="button"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
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
          <span className={styles.aboutText}>TGG v0.1.0</span>
        </div>
      </aside>

      {showSettings && <SettingsModal onClose={() => setShowSettings(false)} />}
    </>
  );
}
