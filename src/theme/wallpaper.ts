import { invoke } from "@tauri-apps/api/core";
import { sourceColorFromImage } from "@material/material-color-utilities";

export type WallpaperImage = {
  dataUrl: string;
  width: number;
  height: number;
};

export async function getWallpaperImage(): Promise<WallpaperImage> {
  return invoke<WallpaperImage>("get_wallpaper_image");
}

export async function seedFromDataUrl(dataUrl: string): Promise<string> {
  const image = new Image();
  image.src = dataUrl;
  await image.decode();
  const argb = await sourceColorFromImage(image);
  return `#${(argb & 0xffffff).toString(16).padStart(6, "0")}`;
}
