import { useState, useRef, useEffect } from 'react';
import { Check, MessageSquare, Pencil, SquarePen, Trash2, X } from 'lucide-react';
import clsx from 'clsx';
import { useChatStore } from '../../stores/chat';
import { relativeDay } from '../../lib/relativeTime';
import { shortcutLabel } from '../../lib/shortcuts';
import styles from './ConversationSwitcher.module.css';

interface ConversationSwitcherProps {
  onClose: () => void;
}

/** Searchable chat list: ↑/↓ to move, Enter to open, Esc to close. */
export function ConversationSwitcher({ onClose }: ConversationSwitcherProps) {
  const [search, setSearch] = useState('');
  const [highlight, setHighlight] = useState(0);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState('');
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const {
    conversations,
    activeConversationId,
    setActiveConversation,
    startNewChat,
    deleteConversation,
    renameConversation,
  } = useChatStore();

  useEffect(() => {
    searchRef.current?.focus();
  }, []);

  const query = search.trim().toLowerCase();
  const filtered = query
    ? conversations.filter((c) => c.title.toLowerCase().includes(query))
    : conversations;

  useEffect(() => {
    setHighlight(0);
  }, [query]);

  useEffect(() => {
    listRef.current
      ?.querySelector<HTMLElement>(`[data-index="${highlight}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }, [highlight]);

  const open = (convId: string) => {
    if (convId !== activeConversationId) setActiveConversation(convId);
    onClose();
  };

  const handleNewChat = () => {
    startNewChat();
    onClose();
  };

  const commitRename = async () => {
    if (editingId && editTitle.trim()) await renameConversation(editingId, editTitle);
    setEditingId(null);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (editingId) return;
    if (e.key === 'Escape') {
      onClose();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      setHighlight((i) => Math.min(i + 1, filtered.length - 1));
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setHighlight((i) => Math.max(i - 1, 0));
    } else if (e.key === 'Enter' && filtered[highlight]) {
      e.preventDefault();
      open(filtered[highlight].id);
    }
  };

  return (
    <>
      <div className={styles.backdrop} onClick={onClose} />
      <div className={styles.dropdown} onKeyDown={handleKeyDown} role="dialog" aria-label="Chats">
        <div className={styles.searchWrapper}>
          <input
            ref={searchRef}
            className={styles.searchInput}
            placeholder="Search chats…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            aria-label="Search chats"
            aria-controls="chat-list"
          />
        </div>

        <ul id="chat-list" ref={listRef} className={styles.list} role="listbox" aria-label="Chats">
          {filtered.length === 0 ? (
            <li className={styles.empty}>{search ? 'No matching chats' : 'No chats yet'}</li>
          ) : (
            filtered.map((conv, index) => (
              <li
                key={conv.id}
                data-index={index}
                role="option"
                aria-selected={conv.id === activeConversationId}
                className={clsx(
                  styles.item,
                  conv.id === activeConversationId && styles.itemActive,
                  index === highlight && styles.itemHighlight,
                )}
                onClick={() => editingId !== conv.id && open(conv.id)}
                onMouseEnter={() => setHighlight(index)}
              >
                <MessageSquare size={14} className={styles.itemIcon} />
                <div className={styles.itemContent}>
                  {editingId === conv.id ? (
                    <input
                      className={styles.renameInput}
                      value={editTitle}
                      autoFocus
                      aria-label="Chat title"
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => setEditTitle(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter') void commitRename();
                        if (e.key === 'Escape') {
                          e.stopPropagation();
                          setEditingId(null);
                        }
                      }}
                      onBlur={() => void commitRename()}
                    />
                  ) : (
                    <div className={styles.itemTitle}>{conv.title}</div>
                  )}
                  <div className={styles.itemMeta}>
                    <span className={styles.itemPreview}>{relativeDay(conv.createdAt)}</span>
                    {(conv.docCount ?? 0) > 0 && (
                      <span className={styles.docCount}>
                        {conv.docCount} doc{conv.docCount !== 1 ? 's' : ''}
                      </span>
                    )}
                  </div>
                </div>
                {editingId !== conv.id && (
                  <div className={styles.itemActions}>
                    {confirmDeleteId === conv.id ? (
                      <>
                        <button
                          type="button"
                          className={clsx(styles.actionBtn, styles.dangerBtn)}
                          onClick={(e) => {
                            e.stopPropagation();
                            setConfirmDeleteId(null);
                            void deleteConversation(conv.id);
                          }}
                          title="Delete chat"
                          aria-label={`Confirm delete ${conv.title}`}
                        >
                          <Check size={12} />
                        </button>
                        <button
                          type="button"
                          className={styles.actionBtn}
                          onClick={(e) => {
                            e.stopPropagation();
                            setConfirmDeleteId(null);
                          }}
                          title="Keep chat"
                          aria-label="Cancel delete"
                        >
                          <X size={12} />
                        </button>
                      </>
                    ) : (
                      <>
                        <button
                          type="button"
                          className={styles.actionBtn}
                          onClick={(e) => {
                            e.stopPropagation();
                            setEditTitle(conv.title);
                            setEditingId(conv.id);
                          }}
                          title="Rename"
                          aria-label={`Rename ${conv.title}`}
                        >
                          <Pencil size={12} />
                        </button>
                        <button
                          type="button"
                          className={styles.actionBtn}
                          onClick={(e) => {
                            e.stopPropagation();
                            setConfirmDeleteId(conv.id);
                          }}
                          title="Delete"
                          aria-label={`Delete ${conv.title}`}
                        >
                          <Trash2 size={12} />
                        </button>
                      </>
                    )}
                  </div>
                )}
              </li>
            ))
          )}
        </ul>

        <button className={styles.newChatBtn} onClick={handleNewChat} type="button">
          <SquarePen size={14} />
          <span>New chat</span>
          <kbd className={styles.kbd}>{shortcutLabel('newChat')}</kbd>
        </button>
      </div>
    </>
  );
}
