import { useEffect } from 'react';
import { useThemeStore } from './stores/theme';
import { Sidebar } from './components/sidebar/Sidebar';
import { ChatPanel } from './components/chat/ChatPanel';
import { PreviewPanel } from './components/preview/PreviewPanel';
import styles from './App.module.css';

function App() {
  const initialize = useThemeStore((s) => s.initialize);

  useEffect(() => {
    initialize();
  }, [initialize]);

  return (
    <div className={styles.layout}>
      <Sidebar />
      <main className={styles.chatPanel}>
        <ChatPanel />
      </main>
      <PreviewPanel />
    </div>
  );
}

export default App;
