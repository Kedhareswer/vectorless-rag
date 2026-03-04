import { invoke } from '@tauri-apps/api/core';

// Types matching Rust structs

export interface DocumentTree {
  id: string;
  name: string;
  doc_type: string;
  root_id: string;
  nodes: Record<string, TreeNode>;
  created_at: string;
  updated_at: string;
}

export interface TreeNode {
  id: string;
  node_type: string;
  content: string;
  metadata: Record<string, unknown>;
  children: string[];
  relations: Relation[];
  summary: string | null;
  raw_image_path: string | null;
}

export interface Relation {
  target_id: string;
  relation_type: string;
  label: string | null;
}

export interface TreeNodeSummary {
  id: string;
  node_type: string;
  content_preview: string;
  children_count: number;
}

export interface DocumentSummary {
  id: string;
  name: string;
  doc_type: string;
  created_at: string;
}

export interface ProviderConfig {
  id: string;
  name: string;
  api_key: string | null;
  base_url: string;
  model: string;
  is_active: boolean;
}

export interface TraceRecord {
  id: string;
  conv_id: string;
  provider_name: string;
  total_tokens: number;
  total_cost: number;
  total_latency_ms: number;
  steps_count: number;
  created_at: string;
}

export interface StepRecord {
  id: string;
  msg_id: string;
  tool_name: string;
  input_json: string;
  output_json: string;
  tokens_used: number;
  latency_ms: number;
}

export interface ConversationRecord {
  id: string;
  title: string;
  doc_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface MessageRecord {
  id: string;
  conv_id: string;
  role: string;
  content: string;
  created_at: string;
}

export interface CostSummaryRecord {
  provider_name: string;
  total_tokens: number;
  total_cost: number;
  query_count: number;
}

// Document commands
export const listDocuments = () => invoke<DocumentSummary[]>('list_documents');
export const getDocument = (id: string) => invoke<DocumentTree>('get_document', { id });
export const ingestDocument = (filePath: string) => invoke<DocumentTree>('ingest_document', { filePath });
export const deleteDocument = (id: string) => invoke<void>('delete_document', { id });

// Tree commands
export const getTreeOverview = (docId: string) => invoke<TreeNodeSummary[]>('get_tree_overview', { docId });
export const expandNode = (docId: string, nodeId: string) => invoke<TreeNode>('expand_node', { docId, nodeId });
export const searchDocument = (docId: string, query: string) => invoke<TreeNode[]>('search_document', { docId, query });

// Provider commands
export const getProviders = () => invoke<ProviderConfig[]>('get_providers');
export const saveProvider = (config: ProviderConfig) => invoke<void>('save_provider', { config });
export const deleteProvider = (id: string) => invoke<void>('delete_provider', { id });

// Settings
export const getSetting = (key: string) => invoke<string | null>('get_setting', { key });
export const setSetting = (key: string, value: string) => invoke<void>('set_setting', { key, value });

// Chat — accepts array of doc IDs for multi-document queries
export const chatWithAgent = (message: string, docIds: string[], providerId: string) =>
  invoke<void>('chat_with_agent', { message, docIds, providerId });

// File dialog
export const openFileDialog = () => invoke<string | null>('open_file_dialog');

// Traces
export const getTraces = (convId: string) => invoke<TraceRecord[]>('get_traces', { convId });
export const getSteps = (msgId: string) => invoke<StepRecord[]>('get_steps', { msgId });
export const getCostSummary = () => invoke<CostSummaryRecord[]>('get_cost_summary');

// Conversations (Feature 1: Chat Persistence)
export const listConversations = () => invoke<ConversationRecord[]>('list_conversations');
export const saveConversationIPC = (id: string, title: string, docId: string | null) =>
  invoke<void>('save_conversation', { id, title, docId });
export const getConversationMessages = (convId: string) => invoke<MessageRecord[]>('get_conversation_messages', { convId });
export const saveMessageIPC = (id: string, convId: string, role: string, content: string) =>
  invoke<void>('save_message', { id, convId, role, content });
export const deleteConversationIPC = (convId: string) => invoke<void>('delete_conversation', { convId });

// Bookmarks
export interface BookmarkRecord {
  id: string;
  doc_id: string;
  node_id: string;
  label: string;
  created_at: string;
}

export const saveBookmark = (docId: string, nodeId: string, label: string) =>
  invoke<void>('save_bookmark', { docId, nodeId, label });
export const getBookmarks = (docId: string) => invoke<BookmarkRecord[]>('get_bookmarks', { docId });
export const deleteBookmark = (id: string) => invoke<void>('delete_bookmark', { id });
