export type TClipboardContentType = "text" | "image" | "file_list";

export interface IClipboardHistoryItem {
  id: number;
  contentType: TClipboardContentType;
  textContent: string | null;
  imagePath: string | null;
  filePaths: string[];
  isPinned: boolean;
  isFavorite: boolean;
  createdAt: string;
  updatedAt: string;
}
