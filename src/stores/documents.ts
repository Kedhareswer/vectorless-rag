import { create } from 'zustand';
import {
  listDocuments,
  getDocument,
  ingestDocument,
  deleteDocument as deleteDocumentIPC,
  type DocumentTree,
  type DocumentSummary as TauriDocumentSummary,
} from '../lib/tauri';

export interface DocumentSummary {
  id: string;
  name: string;
  docType: string;
  createdAt: string;
}

function fromTauriSummary(d: TauriDocumentSummary): DocumentSummary {
  return {
    id: d.id,
    name: d.name,
    docType: d.doc_type,
    createdAt: d.created_at,
  };
}

interface DocumentsState {
  documents: DocumentSummary[];
  activeDocumentId: string | null;
  activeTree: DocumentTree | null;
  /** Multi-select: IDs of all selected documents (Feature 5) */
  selectedDocumentIds: string[];
  isIngesting: boolean;
  isLoadingTree: boolean;
  error: string | null;

  setDocuments: (documents: DocumentSummary[]) => void;
  addDocument: (document: DocumentSummary) => void;
  removeDocument: (id: string) => void;
  setActiveDocument: (id: string | null) => void;
  /** Toggle selection of a document for multi-doc queries (Feature 5) */
  toggleDocumentSelection: (id: string) => void;
  setIsIngesting: (ingesting: boolean) => void;
  loadDocuments: () => Promise<void>;
  ingestDocumentFromPath: (filePath: string) => Promise<void>;
  deleteDocumentFromBackend: (id: string) => Promise<void>;
  loadActiveTree: (docId: string) => Promise<void>;
  clearError: () => void;
}

export const useDocumentsStore = create<DocumentsState>((set) => ({
  documents: [],
  activeDocumentId: null,
  activeTree: null,
  selectedDocumentIds: [],
  isIngesting: false,
  isLoadingTree: false,
  error: null,

  setDocuments: (documents: DocumentSummary[]) => {
    set({ documents });
  },

  addDocument: (document: DocumentSummary) => {
    set((state) => ({
      documents: [...state.documents, document],
    }));
  },

  removeDocument: (id: string) => {
    set((state) => ({
      documents: state.documents.filter((d) => d.id !== id),
      activeDocumentId: state.activeDocumentId === id ? null : state.activeDocumentId,
      activeTree: state.activeTree?.id === id ? null : state.activeTree,
      selectedDocumentIds: state.selectedDocumentIds.filter((sid) => sid !== id),
    }));
  },

  setActiveDocument: (id: string | null) => {
    set({
      activeDocumentId: id,
      activeTree: null,
      selectedDocumentIds: id ? [id] : [],
    });
  },

  toggleDocumentSelection: (id: string) => {
    set((state) => {
      const isSelected = state.selectedDocumentIds.includes(id);
      const newSelection = isSelected
        ? state.selectedDocumentIds.filter((sid) => sid !== id)
        : [...state.selectedDocumentIds, id];
      // Keep activeDocumentId as the first in selection
      const newActive = newSelection.length > 0 ? newSelection[0] : null;
      return {
        selectedDocumentIds: newSelection,
        activeDocumentId: newActive,
        activeTree: newActive !== state.activeDocumentId ? null : state.activeTree,
      };
    });
  },

  setIsIngesting: (ingesting: boolean) => {
    set({ isIngesting: ingesting });
  },

  loadDocuments: async () => {
    try {
      const tauriDocs = await listDocuments();
      const documents = tauriDocs.map(fromTauriSummary);
      set({ documents, error: null });
    } catch (err) {
      console.warn('Failed to load documents from backend:', err);
      set({ error: String(err) });
    }
  },

  ingestDocumentFromPath: async (filePath: string) => {
    set({ isIngesting: true, error: null });
    try {
      const tree = await ingestDocument(filePath);
      const summary: DocumentSummary = {
        id: tree.id,
        name: tree.name,
        docType: tree.doc_type,
        createdAt: tree.created_at,
      };
      set((state) => ({
        documents: [...state.documents, summary],
        activeDocumentId: tree.id,
        activeTree: tree,
        selectedDocumentIds: [tree.id],
        isIngesting: false,
      }));
    } catch (err) {
      console.warn('Failed to ingest document:', err);
      set({ isIngesting: false, error: String(err) });
    }
  },

  deleteDocumentFromBackend: async (id: string) => {
    set({ error: null });
    try {
      await deleteDocumentIPC(id);
      set((state) => ({
        documents: state.documents.filter((d) => d.id !== id),
        activeDocumentId: state.activeDocumentId === id ? null : state.activeDocumentId,
        activeTree: state.activeTree?.id === id ? null : state.activeTree,
        selectedDocumentIds: state.selectedDocumentIds.filter((sid) => sid !== id),
      }));
    } catch (err) {
      console.warn('Failed to delete document:', err);
      set({ error: String(err) });
    }
  },

  loadActiveTree: async (docId: string) => {
    set({ isLoadingTree: true, error: null });
    try {
      const tree = await getDocument(docId);
      set({ activeTree: tree, isLoadingTree: false });
    } catch (err) {
      console.warn('Failed to load document tree:', err);
      set({ activeTree: null, isLoadingTree: false, error: String(err) });
    }
  },

  clearError: () => {
    set({ error: null });
  },
}));
