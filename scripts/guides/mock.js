// Stand-in Tauri backend for recording guides in a plain browser (no Rust, no models).
// Options come from localStorage, set by record.mjs:
//   __servers: 'ollama' (found at start) | 'later' (found once the recorder starts it) | 'none'
//   __tourDone: '1' skips the welcome tour
(() => {
  const servers = localStorage.getItem('__servers') || 'none';
  const now = new Date().toISOString();
  const callbacks = {};
  let nextId = 1;
  let localModelReady = false;

  const settings = localStorage.getItem('__tourDone') === '1' ? { onboarding_done: '1' } : {};
  const providers = [];
  const docs = [];
  const convs = [];
  const convDocs = {};
  const messages = {};

  const contractTree = {
    id: 'd1', name: 'Vendor-Contract-Acme.pdf', doc_type: 'pdf', root_id: 'r', created_at: now, updated_at: now,
    nodes: {
      r: { id: 'r', node_type: 'root', content: 'Vendor-Contract-Acme.pdf', metadata: { page_count: 18 }, children: ['s1', 's2'], relations: [], summary: null, raw_image_path: null },
      s1: { id: 's1', node_type: 'section', content: 'Term and Termination', metadata: { page_number: 9 }, children: [], relations: [], summary: null, raw_image_path: null },
      s2: { id: 's2', node_type: 'section', content: 'Fees and Payment', metadata: { page_number: 4 }, children: [], relations: [], summary: null, raw_image_path: null },
    },
  };

  const send = (channel, message, index) => callbacks[channel.id] && callbacks[channel.id]({ message, index });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const ollama = { provider: 'ollama', label: 'Ollama', base_url: 'http://localhost:11434', models: ['llama3.2:3b', 'qwen3:8b'] };

  const handlers = {
    get_setting: ({ key }) => settings[key] ?? null,
    set_setting: ({ key, value }) => { settings[key] = value; },
    get_providers: () => providers.map((p) => ({ ...p })),
    save_provider: ({ config }) => {
      const i = providers.findIndex((p) => p.id === config.id);
      if (i >= 0) providers[i] = config; else providers.push(config);
    },
    delete_provider: ({ id }) => { const i = providers.findIndex((p) => p.id === id); if (i >= 0) providers.splice(i, 1); },
    detect_local_models: async () => {
      await sleep(400);
      // 'later': Ollama starts while the guide is open (the recorder sets __ollamaRunning).
      if (servers === 'ollama' || (servers === 'later' && window.__ollamaRunning)) return [ollama];
      return [];
    },
    check_local_model: () => ({ downloaded: localModelReady, model_id: localModelReady ? 'lfm2.5-1.2b-instruct-qad-q4' : null, model_path: null, size_bytes: 700e6, tokenizer_ready: localModelReady }),
    get_model_options: () => [
      { id: 'lfm2.5-1.2b-instruct-qad-q4', name: 'LFM2.5 1.2B Instruct', description: 'Answers questions offline and improves search. Recommended.', size_bytes: 700e6, size_label: '~700 MB', url: '', filename: '', sha256: null, tokenizer_url: '' },
      { id: 'lfm2.5-230m-qad-q4', name: 'LFM2.5 230M (lightweight)', description: 'Search help only — too small for reliable answers. For low-memory PCs.', size_bytes: 150e6, size_label: '~150 MB', url: '', filename: '', sha256: null, tokenizer_url: '' },
    ],
    download_local_model: async ({ onProgress }) => {
      const total = 700e6;
      for (let i = 0; i <= 20; i++) {
        const percent = i * 5;
        send(onProgress, { downloaded_bytes: (total * percent) / 100, total_bytes: total, percent, done: i === 20, error: null, phase: 'Downloading LFM2.5 1.2B Instruct…' }, i);
        await sleep(150);
      }
      localModelReady = true;
    },
    check_dictation_model: () => ({ downloaded: false, model_id: null, loaded: false }),
    get_dictation_options: () => [],
    list_documents: () => docs.map((d) => ({ ...d })),
    get_document: () => contractTree,
    open_file_dialog: () => 'C:/Users/me/Documents/Vendor-Contract-Acme.pdf',
    ingest_document: async () => {
      await sleep(900);
      docs.push({ id: 'd1', name: contractTree.name, doc_type: 'pdf', created_at: now });
      return contractTree;
    },
    list_conversations: () => convs.map((c) => ({ ...c })),
    save_conversation: ({ id, title }) => {
      const c = convs.find((x) => x.id === id);
      if (c) c.title = title; else convs.unshift({ id, title, doc_id: null, created_at: now, updated_at: now });
    },
    rename_conversation: ({ convId, title }) => { const c = convs.find((x) => x.id === convId); if (c) c.title = title; },
    get_conversation_messages: ({ convId }) => messages[convId] || [],
    save_message: ({ id, convId, role, content }) => { (messages[convId] ||= []).push({ id, conv_id: convId, role, content, created_at: now }); },
    add_doc_to_conversation: ({ convId, docId }) => { (convDocs[convId] ||= []).push(docId); },
    get_conversation_doc_ids: ({ convId }) => convDocs[convId] || [],
    get_traces: () => [],
    get_cost_summary: () => [],
    get_cross_doc_relations: () => [],
    get_bookmarks: () => [],
    get_tree_overview: () => [],
    get_rich_overview: () => [],
    check_liteparse_available: () => true,
    'plugin:opener|open_url': () => null,
    chat_with_agent: async ({ onEvent }) => {
      let index = 0;
      const emit = (message) => send(onEvent, message, index++);
      const active = providers.find((p) => p.is_active);
      const steps = [
        ['query_rewrite', 'Rephrasing query', 'contract renewal notice period termination clause'],
        ['hyde', 'Generating hypothetical answer', 'The agreement renews automatically unless either party gives notice.'],
        ['stepback', 'Broadening question', 'What are the renewal and termination provisions?'],
        ['search', 'Searching 11 terms across 1 document', '6 matching nodes'],
        ['expand', 'Reading section "Term and Termination"', 'This Agreement shall renew automatically for successive twelve (12) month periods…'],
      ];
      let n = 1;
      for (const [tool, input, output] of steps) {
        emit({ type: 'step-start', stepNumber: n, tool, inputSummary: input });
        await sleep(450);
        emit({ type: 'step-complete', stepNumber: n, outputSummary: output, tokensUsed: 0, latencyMs: 420, cost: 0, nodeIds: [] });
        n++;
      }
      emit({ type: 'step-start', stepNumber: n, tool: 'llm_call', inputSummary: `Generating answer with ${active ? active.model : 'model'}` });
      const answer = 'The contract **renews automatically** for 12-month terms (§9.1). To stop renewal, either party must give **90 days written notice** before the term ends (§9.2).\n\n- Early termination for convenience costs **3 months of fees** (§9.4).\n- Termination for breach needs a 30-day cure period (§9.5).';
      for (const word of answer.split(/(?<= )/)) {
        emit({ type: 'token', token: word, done: false });
        await sleep(35);
      }
      emit({ type: 'step-complete', stepNumber: n, outputSummary: '2,140 tokens · free', tokensUsed: 2140, latencyMs: 2600, cost: 0, nodeIds: [] });
      emit({ type: 'response', content: answer });
      return null;
    },
  };

  window.__TAURI_INTERNALS__ = {
    metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
    transformCallback: (cb) => { const id = nextId++; callbacks[id] = cb; return id; },
    unregisterCallback: (id) => { delete callbacks[id]; },
    convertFileSrc: (p) => p,
    invoke: async (cmd, args) => {
      const h = handlers[cmd];
      if (cmd === 'plugin:event|listen') return nextId++;
      return h ? h(args || {}) : null;
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
})();
