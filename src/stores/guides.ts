import { create } from 'zustand';
import type { ModelPath } from '../lib/modelGuides';

interface GuidesState {
  /** First-run welcome tour. */
  tourOpen: boolean;
  /** "Add a model" guide; `addModelPath` is the option shown first (null = choose). */
  addModelOpen: boolean;
  addModelPath: ModelPath | null;
  openTour: () => void;
  closeTour: () => void;
  /** Show the add-model guide, optionally on one option. Opened when a model is first needed. */
  openAddModel: (path?: ModelPath) => void;
  closeAddModel: () => void;
}

export const useGuidesStore = create<GuidesState>((set) => ({
  tourOpen: false,
  addModelOpen: false,
  addModelPath: null,
  openTour: () => set({ tourOpen: true }),
  closeTour: () => set({ tourOpen: false }),
  openAddModel: (path) => set({ addModelOpen: true, addModelPath: path ?? null }),
  closeAddModel: () => set({ addModelOpen: false, addModelPath: null }),
}));
