import { signal } from "@preact/signals-react";

export const userMovies = signal<any[]>([]);
export const userAudioItems = signal<any[]>([]);
export const isRetreivingAudioItems = signal<boolean>(false);
export const isRetreivingUserMovies = signal<boolean>(false);

export function setUserMovies(movies: any[]) {
  userMovies.value = movies;
}

export function setUserAudioItems(audios: any[]) {
  userAudioItems.value = audios;
}
