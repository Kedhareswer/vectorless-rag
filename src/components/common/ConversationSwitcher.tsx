import { useState, useRef, useEffect, useCallback } from 'react';
import { Plus, Trash2, MessageSquare } from 'lucide-react';
import clsx from 'clsx';
import { useChatStore } from '../../stores/chat';
import styles from './ConversationSwitcher.module.css';

interface ConversationSwitcherProps {
  onClose: () => void;
}

export function ConversationSwitcher({ onClose }: ConversationSwitcherProps) {
  const [search, setSearch] = useState('');
  const searchRef = useRef<HTMLInputElement>(null);
  const {
    conversations,
    activeConversationId,
    setActiveConversation,
    createConversation,
    deleteConversation,
  } = useChatStore();

  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (e.key === 'Escape') onClose();
  }, [onClose]);

  const handleSelect = (convId: string) => {
    setActiveConversation(convId);
    onClose();
  };

  const handleNewChat = () => {
    createConversation('New Chat');
    onClose();
  };

  const handleDelete = async (e: React.MouseEvent, convId: string) => {
    e.stopPropagation();
    await deleteConversation(convId);
  };

  const filtered = search.trim()
    ? conversations.filter((c) =>
        c.title.toLowerCase().includes(search.toLowerCase())
      )
    : conversations;

  return (
    <>
      <div className={styles.backdrop} onClick={onClose} />
      <div className={styles.dropdown} onKeyDown={handleKeyDown}>
        <div className={styles.searchWrapper}>
          <input
            ref={searchRef}
            className={styles.searchInput}
            placeholder="Search chats..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>

        <div className={styles.list}>
          {filtered.length === 0 ? (
            <div className={styles.empty}>
              {search ? 'No matching chats' : 'No conversations yet'}
            </div>
          ) : (
            filtered.map((conv) => (
              <div
                key={conv.id}
                className={clsx(
                  styles.item,
                  activeConversationId === conv.id && styles.itemActive,
                )}
                onClick={() => handleSelect(conv.id)}
              >
                <MessageSquare size={14} style={{ color: 'var(--text-tertiary)', flexShrink: 0, marginTop: 2 }} />
                <div className={styles.itemContent}>
                  <div className={styles.itemTitle}>{conv.title}</div>
                  <div className={styles.itemMeta}>
                    <span className={styles.itemPreview}>
                      {conv.createdAt ? new Date(conv.createdAt).toLocaleDateString() : ''}
                    </span>
                    {(conv.docCount ?? 0) > 0 && (
                      <span className={styles.docCount}>
                        {conv.docCount} doc{conv.docCount !== 1 ? 's' : ''}
                      </span>
                    )}
                  </div>
                </div>
                <button
                  className={styles.deleteBtn}
                  onClick={(e) => handleDelete(e, conv.id)}
                  title="Delete conversation"
                  type="button"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            ))
          )}
        </div>

        <button className={styles.newChatBtn} onClick={handleNewChat} type="button">
          <Plus size={14} />
          <span>New Chat</span>
        </button>
      </div>
    </>
  );
}
