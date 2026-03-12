import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useDocumentsStore, type DocumentSummary } from './documents';

vi.mock('../lib/tauri', () => ({
  listDocuments: vi.fn().mockResolvedValue([]),
  getDocument: vi.fn().mockResolvedValue(null),
  ingestDocument: vi.fn().mockResolvedValue(undefined),
  deleteDocument: vi.fn().mockResolvedValue(undefined),
}));

const initialState = () => ({
  documents: [],
  activeDocumentId: null,
  activeTree: null,
  isIngesting: false,
  isLoadingTree: false,
  error: null,
});

describe('DocumentsStore', () => {
  beforeEach(() => {
    useDocumentsStore.setState(initialState());
  });

  // ── Initial state ──────────────────────────────────────────────

  it('has correct initial state', () => {
    const state = useDocumentsStore.getState();
    expect(state.documents).toEqual([]);
    expect(state.activeDocumentId).toBeNull();
    expect(state.activeTree).toBeNull();
    expect(state.isIngesting).toBe(false);
    expect(state.isLoadingTree).toBe(false);
    expect(state.error).toBeNull();
  });

  // ── setDocuments ───────────────────────────────────────────────

  it('setDocuments replaces the documents list', () => {
    const docs = [makeDoc('d1', 'File A'), makeDoc('d2', 'File B')];
    useDocumentsStore.getState().setDocuments(docs);

    expect(useDocumentsStore.getState().documents).toEqual(docs);
  });

  it('setDocuments with empty array clears documents', () => {
    useDocumentsStore.setState({ documents: [makeDoc('d1', 'X')] });
    useDocumentsStore.getState().setDocuments([]);

    expect(useDocumentsStore.getState().documents).toEqual([]);
  });

  // ── addDocument ────────────────────────────────────────────────

  it('addDocument appends to the list', () => {
    useDocumentsStore.setState({ documents: [makeDoc('d1', 'Existing')] });
    useDocumentsStore.getState().addDocument(makeDoc('d2', 'New'));

    const docs = useDocumentsStore.getState().documents;
    expect(docs).toHaveLength(2);
    expect(docs[1].id).toBe('d2');
  });

  // ── removeDocument ─────────────────────────────────────────────

  it('removeDocument filters out the document', () => {
    useDocumentsStore.setState({
      documents: [makeDoc('d1', 'A'), makeDoc('d2', 'B')],
    });

    useDocumentsStore.getState().removeDocument('d1');

    const docs = useDocumentsStore.getState().documents;
    expect(docs).toHaveLength(1);
    expect(docs[0].id).toBe('d2');
  });

  it('removeDocument clears activeDocumentId when removing the active document', () => {
    useDocumentsStore.setState({
      documents: [makeDoc('d1', 'A')],
      activeDocumentId: 'd1',
      activeTree: { id: 'd1', name: 'A', doc_type: 'pdf', root_id: 'r', nodes: {}, created_at: '', updated_at: '' },
    });

    useDocumentsStore.getState().removeDocument('d1');

    const state = useDocumentsStore.getState();
    expect(state.activeDocumentId).toBeNull();
    expect(state.activeTree).toBeNull();
  });

  it('removeDocument does not clear activeDocumentId when removing a different document', () => {
    useDocumentsStore.setState({
      documents: [makeDoc('d1', 'A'), makeDoc('d2', 'B')],
      activeDocumentId: 'd2',
    });

    useDocumentsStore.getState().removeDocument('d1');

    expect(useDocumentsStore.getState().activeDocumentId).toBe('d2');
  });

  // ── setActiveDocument ──────────────────────────────────────────

  it('setActiveDocument sets activeDocumentId and clears tree', () => {
    useDocumentsStore.getState().setActiveDocument('d1');

    const state = useDocumentsStore.getState();
    expect(state.activeDocumentId).toBe('d1');
    expect(state.activeTree).toBeNull();
  });

  it('setActiveDocument with null clears selection', () => {
    useDocumentsStore.setState({
      activeDocumentId: 'd1',
      activeTree: { id: 'd1', name: 'A', doc_type: 'pdf', root_id: 'r', nodes: {}, created_at: '', updated_at: '' },
    });

    useDocumentsStore.getState().setActiveDocument(null);

    const state = useDocumentsStore.getState();
    expect(state.activeDocumentId).toBeNull();
    expect(state.activeTree).toBeNull();
  });

  // ── setIsIngesting ─────────────────────────────────────────────

  it('setIsIngesting toggles the flag', () => {
    useDocumentsStore.getState().setIsIngesting(true);
    expect(useDocumentsStore.getState().isIngesting).toBe(true);

    useDocumentsStore.getState().setIsIngesting(false);
    expect(useDocumentsStore.getState().isIngesting).toBe(false);
  });

  // ── clearError ─────────────────────────────────────────────────

  it('clearError sets error to null', () => {
    useDocumentsStore.setState({ error: 'Something failed' });

    useDocumentsStore.getState().clearError();

    expect(useDocumentsStore.getState().error).toBeNull();
  });

  // ── deleteDocumentFromBackend ──────────────────────────────────

  it('deleteDocumentFromBackend removes the document from state', async () => {
    useDocumentsStore.setState({
      documents: [makeDoc('d1', 'A'), makeDoc('d2', 'B')],
      activeDocumentId: 'd1',
    });

    await useDocumentsStore.getState().deleteDocumentFromBackend('d1');

    const state = useDocumentsStore.getState();
    expect(state.documents).toHaveLength(1);
    expect(state.documents[0].id).toBe('d2');
    expect(state.activeDocumentId).toBeNull();
  });

  it('deleteDocumentFromBackend does not affect other active document', async () => {
    useDocumentsStore.setState({
      documents: [makeDoc('d1', 'A'), makeDoc('d2', 'B')],
      activeDocumentId: 'd2',
    });

    await useDocumentsStore.getState().deleteDocumentFromBackend('d1');

    const state = useDocumentsStore.getState();
    expect(state.activeDocumentId).toBe('d2');
  });

  // ── ingestDocumentFromPath ─────────────────────────────────────

  it('ingestDocumentFromPath sets isIngesting during operation', async () => {
    const { ingestDocument } = await import('../lib/tauri');
    const tree = {
      id: 'new-doc',
      name: 'uploaded.pdf',
      doc_type: 'pdf',
      root_id: 'r1',
      nodes: {},
      created_at: '2025-01-01',
      updated_at: '2025-01-01',
    };
    vi.mocked(ingestDocument).mockResolvedValueOnce(tree);

    const result = await useDocumentsStore.getState().ingestDocumentFromPath('/path/to/file.pdf');

    const state = useDocumentsStore.getState();
    expect(state.isIngesting).toBe(false);
    expect(state.documents).toHaveLength(1);
    expect(state.documents[0].id).toBe('new-doc');
    expect(state.activeDocumentId).toBe('new-doc');
    expect(state.activeTree).toEqual(tree);
    expect(result).not.toBeNull();
    expect(result?.id).toBe('new-doc');
  });

  it('ingestDocumentFromPath sets error on failure', async () => {
    const { ingestDocument } = await import('../lib/tauri');
    vi.mocked(ingestDocument).mockRejectedValueOnce(new Error('Parse failed'));

    await useDocumentsStore.getState().ingestDocumentFromPath('/bad/file.xyz');

    const state = useDocumentsStore.getState();
    expect(state.isIngesting).toBe(false);
    expect(state.error).toBe('Error: Parse failed');
  });
});

// ── Helpers ────────────────────────────────────────────────────────

function makeDoc(id: string, name: string): DocumentSummary {
  return { id, name, docType: 'pdf', createdAt: '2025-01-01T00:00:00Z' };
}
