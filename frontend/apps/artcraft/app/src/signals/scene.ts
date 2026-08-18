import { signal } from "@preact/signals-react";

export const scene = {
  activeSceneToken: signal<string | null>(null),
  activeSceneTitle: signal<string>("Untitled Scene"),
  scenes: signal<any[]>([]),
  isLoading: signal<boolean>(false),
};

export const signalScene = scene;
