import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, ReactElement } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import { HISTORY_UPDATED_EVENT, LAUNCHER_SHOWN_EVENT } from "./constants/history";
import type { IClipboardHistoryItem, TClipboardContentType } from "./types/clipboard";
import { openPath } from '@tauri-apps/plugin-opener';

type THistoryStatus = "idle" | "loading" | "refreshing";
type TListGroupKey = "all" | "favorite" | TClipboardContentType | "link";
type TListSortKey = "latest" | "oldest";
type TErrorTitle =
  | "加载失败"
  | "刷新失败"
  | "删除失败"
  | "清空失败"
  | "置顶失败"
  | "收藏失败"
  | "关闭失败"
  | "复制失败"
  | "粘贴失败";

interface IErrorState {
  title: TErrorTitle;
  message: string;
}

const DATE_TIME_FORMATTER = new Intl.DateTimeFormat("zh-CN", {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
});

const CONTENT_TYPE_LABELS: Record<TClipboardContentType, string> = {
  text: "文本",
  image: "图片",
  file_list: "文件",
};

const CONTENT_TYPE_BADGES: Record<TClipboardContentType, string> = {
  text: "T",
  image: "IMG",
  file_list: "F",
};

const LIST_GROUP_OPTIONS: Array<{ key: TListGroupKey; label: string }> = [
  { key: "all", label: "全部" },
  { key: "favorite", label: "收藏" },
  { key: "text", label: "文本" },
  { key: "image", label: "图片" },
  { key: "file_list", label: "文件" },
  { key: "link", label: "链接" },
];

const SORT_OPTION_LABELS: Record<TListSortKey, string> = {
  latest: "最新优先",
  oldest: "最早优先",
};

const EMPTY_TEXT_MESSAGE = "这条文本记录为空。";
const EMPTY_IMAGE_MESSAGE = "图片文件不可用。";
const KEY_REPEAT_INITIAL_DELAY_MS = 140;
const KEY_REPEAT_INTERVAL_MS = 45;

const SearchIcon = (): ReactElement => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M10.5 4.5a6 6 0 1 0 0 12a6 6 0 0 0 0-12m0-1.5a7.5 7.5 0 1 1 0 15a7.5 7.5 0 0 1 0-15m10.03 16.97a.75.75 0 1 1-1.06 1.06l-4.1-4.1a.75.75 0 0 1 1.06-1.06z"
      fill="currentColor"
    />
  </svg>
);

const PinIcon = ({ filled = false }: { filled?: boolean }): ReactElement => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M14.92 3.93a.75.75 0 0 1 1.06 0l4.09 4.09a.75.75 0 0 1 0 1.06l-1.86 1.86a1.75 1.75 0 0 0-.46.82l-.59 2.35a1.75 1.75 0 0 1-.46.82l-1.3 1.3a.75.75 0 0 1-1.06 0l-1.57-1.57l-4.74 4.74a.75.75 0 1 1-1.06-1.06l4.74-4.74l-1.57-1.57a.75.75 0 0 1 0-1.06l1.3-1.3c.22-.22.38-.5.46-.82l.59-2.35c.08-.31.23-.59.46-.82zm.53 1.59-1.87 1.87a3.26 3.26 0 0 0-.85 1.5l-.59 2.36a.25.25 0 0 1-.07.12l-.77.77l3.04 3.04l.77-.77a.25.25 0 0 1 .12-.07l2.36-.59c.57-.14 1.09-.44 1.5-.85l1.87-1.87z"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinejoin="round"
      strokeLinecap="round"
    />
  </svg>
);

const StarIcon = ({ filled = false }: { filled?: boolean }): ReactElement => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M11.33 4.83a.75.75 0 0 1 1.34 0l1.84 3.73l4.12.6a.75.75 0 0 1 .42 1.28l-2.98 2.9l.7 4.1a.75.75 0 0 1-1.09.79L12 16.31l-3.68 1.92a.75.75 0 0 1-1.09-.79l.7-4.1l-2.98-2.9a.75.75 0 0 1 .42-1.28l4.12-.6z"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinejoin="round"
    />
  </svg>
);

const TrashIcon = (): ReactElement => (
  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024"><path fill="currentColor" d="M160 256H96a32 32 0 0 1 0-64h256V96a32 32 0 0 1 32-32h256a32 32 0 0 1 32 32v96h256a32 32 0 1 1 0 64h-64v672a32 32 0 0 1-32 32H192a32 32 0 0 1-32-32zm448-64v-64H416v64zM224 896h576V256H224zm192-128a32 32 0 0 1-32-32V416a32 32 0 0 1 64 0v320a32 32 0 0 1-32 32m192 0a32 32 0 0 1-32-32V416a32 32 0 0 1 64 0v320a32 32 0 0 1-32 32"/></svg>
);

const getPathBasename = (value: string | null): string => {
  if (!value) {
    return EMPTY_IMAGE_MESSAGE;
  }

  const normalizedValue = value.replace(/\\/g, "/");
  const segments = normalizedValue.split("/");
  return segments[segments.length - 1] || value;
};

const getFileListDisplayText = (filePaths: string[]): string => {
  if (filePaths.length === 0) {
    return "暂无文件路径";
  }

  return filePaths.map((filePath) => getPathBasename(filePath)).join("\n");
};

const formatTimestamp = (value: string): string => {
  const parsedValue = new Date(value);

  if (Number.isNaN(parsedValue.getTime())) {
    return value;
  }

  return DATE_TIME_FORMATTER.format(parsedValue);
};

const parseTimestamp = (value: string): number => {
  const parsedValue = new Date(value).getTime();
  return Number.isNaN(parsedValue) ? 0 : parsedValue;
};

const isLinkValue = (value: string | null): boolean =>
  typeof value === "string" && /^https?:\/\/\S+$/i.test(value.trim());

const isLinkHistoryItem = (item: IClipboardHistoryItem): boolean =>
  item.contentType === "text" && isLinkValue(item.textContent);

const isFileHistoryItem = (item: IClipboardHistoryItem): boolean =>
  item.contentType === "file_list" && item.filePaths.length > 0;

const buildSearchSpace = (item: IClipboardHistoryItem): string =>
  [item.textContent ?? "", item.imagePath ?? "", ...item.filePaths].join("\n").toLowerCase();

const filterHistoryItems = (items: IClipboardHistoryItem[], query: string): IClipboardHistoryItem[] => {
  const normalizedQuery = query.trim().toLowerCase();
  return normalizedQuery ? items.filter((item) => buildSearchSpace(item).includes(normalizedQuery)) : items;
};

const filterHistoryItemsByGroup = (
  items: IClipboardHistoryItem[],
  group: TListGroupKey,
): IClipboardHistoryItem[] => {
  if (group === "all") {
    return items;
  }

  if (group === "favorite") {
    return items.filter((item) => item.isFavorite);
  }

  if (group === "link") {
    return items.filter(isLinkHistoryItem);
  }

  return items.filter((item) => item.contentType === group);
};

const sortHistoryItems = (
  items: IClipboardHistoryItem[],
  sortKey: TListSortKey,
): IClipboardHistoryItem[] => {
  return [...items].sort((leftItem, rightItem) => {
    if (leftItem.isPinned !== rightItem.isPinned) {
      return leftItem.isPinned ? -1 : 1;
    }

    const leftTimestamp = parseTimestamp(leftItem.updatedAt);
    const rightTimestamp = parseTimestamp(rightItem.updatedAt);
    const timeDiff = sortKey === "latest" ? rightTimestamp - leftTimestamp : leftTimestamp - rightTimestamp;

    if (timeDiff !== 0) {
      return timeDiff;
    }

    return rightItem.id - leftItem.id;
  });
};

const getListGroupCount = (items: IClipboardHistoryItem[], group: TListGroupKey): number =>
  filterHistoryItemsByGroup(items, group).length;

const getItemTypeLabel = (item: IClipboardHistoryItem): string =>
  isLinkHistoryItem(item) ? "链接" : CONTENT_TYPE_LABELS[item.contentType];

const FileIcon = (): ReactElement => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M13 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V9zm0 0v6h6"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const LinkIcon = (): ReactElement => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71m3.25 6.82a5 5 0 0 0-7.54.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const getItemBadgeText = (item: IClipboardHistoryItem): ReactElement =>
  isLinkHistoryItem(item) ? <LinkIcon /> : isFileHistoryItem(item) ? <FileIcon /> : <span>{CONTENT_TYPE_BADGES[item.contentType]}</span>;

const getVisibleHistoryItems = (
  items: IClipboardHistoryItem[],
  query: string,
  group: TListGroupKey,
  sortKey: TListSortKey,
): IClipboardHistoryItem[] => {
  const groupedHistory = filterHistoryItemsByGroup(items, group);
  const searchedHistory = filterHistoryItems(groupedHistory, query);
  return sortHistoryItems(searchedHistory, sortKey);
};

const getItemPrimaryText = (item: IClipboardHistoryItem): string => {
  if (item.contentType === "text") {
    return item.textContent?.trim() || EMPTY_TEXT_MESSAGE;
  }

  if (item.contentType === "image") {
    return getPathBasename(item.imagePath);
  }

  return getFileListDisplayText(item.filePaths);
};

const getVisibleItemCountText = (items: IClipboardHistoryItem[], isRefreshing: boolean): string => {
  if (isRefreshing) {
    return "刷新中...";
  }

  return `${items.length} 条`;
};

const renderListPreview = (item: IClipboardHistoryItem): ReactElement => {
  if (item.contentType === "image" && item.imagePath) {
    return (
      <div className="item-preview item-preview-inline">
        <img className="history-image" src={convertFileSrc(item.imagePath)} alt="Clipboard preview" />
        <div className="item-preview-copy">
          <p className="item-primary">{getItemPrimaryText(item)}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="item-preview item-preview-stack">
      <p className="item-primary">{getItemPrimaryText(item)}</p>
    </div>
  );
};

const renderPreviewContent = (item: IClipboardHistoryItem, previewRef: React.RefObject<HTMLPreElement | null>): ReactElement => {
  if (item.contentType === "image" && item.imagePath) {
    return (
      <div className="preview-content preview-content-image">
        <img onClick={() => {
          item.imagePath && openPath(item.imagePath)
        }} className="preview-image" src={convertFileSrc(item.imagePath)} alt="Clipboard preview" />
        <pre className="preview-text">{item.imagePath || EMPTY_IMAGE_MESSAGE}</pre>
      </div>
    );
  }

  if (item.contentType === "file_list") {
    return (
      <pre onClick={() => {
        void openPath(item.filePaths.join("\n"))
      }} className="preview-text preview-link">{item.filePaths.length > 0 ? item.filePaths.join("\n") : "暂无文件路径"}</pre>
    );
  }

  const content = item.textContent?.trim()
  if (content && isLinkValue(content)) {
    // 如果是链接，可以点击
    return <pre className="preview-link preview-text" onClick={() => {
      void openPath(content)
    }}>{content}</pre>;
  }

  return <pre ref={previewRef} contentEditable suppressContentEditableWarning={true} className="preview-text">{content || EMPTY_TEXT_MESSAGE}</pre>;
};

const App = (): ReactElement => {
  const [history, setHistory] = useState<IClipboardHistoryItem[]>([]);
  const [query, setQuery] = useState<string>("");
  const [activeGroup, setActiveGroup] = useState<TListGroupKey>("all");
  const [activeSort, setActiveSort] = useState<TListSortKey>("latest");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [errorState, setErrorState] = useState<IErrorState | null>(null);
  const [historyStatus, setHistoryStatus] = useState<THistoryStatus>("idle");
  const [activeDeleteId, setActiveDeleteId] = useState<number | null>(null);
  const [activeCopyId, setActiveCopyId] = useState<number | null>(null);
  const [activePinId, setActivePinId] = useState<number | null>(null);
  const [activeFavoriteId, setActiveFavoriteId] = useState<number | null>(null);
  const [isClearing, setIsClearing] = useState<boolean>(false);
  const latestStartedLoadRequestIdRef = useRef<number>(0);
  const latestAppliedLoadRequestIdRef = useRef<number>(0);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const launcherListRef = useRef<HTMLElement | null>(null);
  const latestHistoryRef = useRef<IClipboardHistoryItem[]>([]);
  const latestQueryRef = useRef<string>("");
  const latestGroupRef = useRef<TListGroupKey>("all");
  const latestSortRef = useRef<TListSortKey>("latest");
  const latestSelectedIdRef = useRef<number | null>(null);
  const shouldResetSelectionRef = useRef<boolean>(true);
  const keyRepeatTimeoutRef = useRef<number | null>(null);
  const keyRepeatIntervalRef = useRef<number | null>(null);
  const activeArrowKeyRef = useRef<"ArrowDown" | "ArrowUp" | null>(null);

  const formatErrorMessage = (error: unknown): string =>
    error instanceof Error ? error.message : String(error);

  const loadHistory = async (
    status: THistoryStatus = "refreshing",
    errorTitle: TErrorTitle = "刷新失败",
  ): Promise<boolean> => {
    const requestId = latestStartedLoadRequestIdRef.current + 1;
    latestStartedLoadRequestIdRef.current = requestId;
    setHistoryStatus(status);

    try {
      const items = await invoke<IClipboardHistoryItem[]>("list_clipboard_history");

      if (requestId < latestAppliedLoadRequestIdRef.current) {
        return false;
      }

      latestAppliedLoadRequestIdRef.current = requestId;
      setHistory(items);
      setErrorState(null);
      return true;
    } catch (error) {
      if (requestId !== latestStartedLoadRequestIdRef.current) {
        return false;
      }

      setErrorState({
        title: errorTitle,
        message: formatErrorMessage(error),
      });
      return false;
    } finally {
      if (requestId === latestStartedLoadRequestIdRef.current) {
        setHistoryStatus("idle");
      }
    }
  };

  const focusSearchInput = (): void => {
    window.requestAnimationFrame(() => {
      searchInputRef.current?.focus();
    });
  };

  const clearArrowKeyRepeat = (): void => {
    if (keyRepeatTimeoutRef.current !== null) {
      window.clearTimeout(keyRepeatTimeoutRef.current);
      keyRepeatTimeoutRef.current = null;
    }

    if (keyRepeatIntervalRef.current !== null) {
      window.clearInterval(keyRepeatIntervalRef.current);
      keyRepeatIntervalRef.current = null;
    }

    activeArrowKeyRef.current = null;
  };

  const moveSelectionByOffset = (offset: number): void => {
    const visibleItems = getVisibleHistoryItems(
      latestHistoryRef.current,
      latestQueryRef.current,
      latestGroupRef.current,
      latestSortRef.current,
    );

    if (visibleItems.length === 0) {
      return;
    }

    const currentIndex =
      latestSelectedIdRef.current === null
        ? -1
        : visibleItems.findIndex((item) => item.id === latestSelectedIdRef.current);
    const nextIndex =
      currentIndex < 0
        ? 0
        : (currentIndex + offset + visibleItems.length) % visibleItems.length;

    setSelectedId(visibleItems[nextIndex]?.id ?? null);
  };

  const resetLauncherView = (): void => {
    const nextItems = getVisibleHistoryItems(
      latestHistoryRef.current,
      latestQueryRef.current,
      latestGroupRef.current,
      latestSortRef.current,
    );

    setErrorState(null);
    shouldResetSelectionRef.current = true;
    setSelectedId(nextItems[0]?.id ?? null);
    focusSearchInput();

    window.requestAnimationFrame(() => {
      launcherListRef.current?.scrollTo({ top: 0, behavior: "auto" });
    });
  };

  const hideLauncherWindow = async (): Promise<boolean> => {
    try {
      await invoke("hide_launcher_window");
      return true;
    } catch (error) {
      setErrorState({
        title: "关闭失败",
        message: formatErrorMessage(error),
      });
      return false;
    }
  };

  const copyHistoryItem = async (item: IClipboardHistoryItem): Promise<void> => {
    if (activeCopyId !== null || activeDeleteId !== null || activePinId !== null || activeFavoriteId !== null || isClearing) {
      return;
    }

    setActiveCopyId(item.id);

    try {
      await invoke("copy_clipboard_history", { id: item.id });
      await hideLauncherWindow();
    } catch (error) {
      setErrorState({
        title: "复制失败",
        message: formatErrorMessage(error),
      });
    } finally {
      setActiveCopyId(null);
    }
  };

  const pasteHistoryItem = async (item: IClipboardHistoryItem): Promise<void> => {
    if (activeCopyId !== null || activeDeleteId !== null || activePinId !== null || activeFavoriteId !== null || isClearing) {
      return;
    }

    setActiveCopyId(item.id);

    try {
      await invoke("paste_history_into_previous_app", { id: item.id });
    } catch (error) {
      setErrorState({
        title: "粘贴失败",
        message: formatErrorMessage(error),
      });
    } finally {
      setActiveCopyId(null);
    }
  };

  const handleDelete = async (id: number): Promise<void> => {
    setActiveDeleteId(id);

    try {
      await invoke("delete_clipboard_history", { id });
      setHistory((currentHistory) => currentHistory.filter((item) => item.id !== id));
      await loadHistory("refreshing", "刷新失败");
    } catch (error) {
      setErrorState({
        title: "删除失败",
        message: formatErrorMessage(error),
      });
    } finally {
      setActiveDeleteId(null);
    }
  };

  const handleClearHistory = async (): Promise<void> => {
    if (isClearing || history.every((item) => item.isFavorite)) {
      return;
    }

    setIsClearing(true);

    try {
      await invoke("clear_clipboard_history");
      await loadHistory("refreshing", "刷新失败");
    } catch (error) {
      setErrorState({
        title: "清空失败",
        message: formatErrorMessage(error),
      });
    } finally {
      setIsClearing(false);
    }
  };

  const handleTogglePin = async (id: number): Promise<void> => {
    if (isMutating) {
      return;
    }

    setActivePinId(id);

    try {
      await invoke("toggle_pin_clipboard_history", { id });
      await loadHistory("refreshing", "刷新失败");
    } catch (error) {
      setErrorState({
        title: "置顶失败",
        message: formatErrorMessage(error),
      });
    } finally {
      setActivePinId(null);
    }
  };

  const handleToggleFavorite = async (id: number): Promise<void> => {
    if (isMutating) {
      return;
    }

    setActiveFavoriteId(id);

    try {
      await invoke("toggle_favorite_clipboard_history", { id });
      await loadHistory("refreshing", "刷新失败");
    } catch (error) {
      setErrorState({
        title: "收藏失败",
        message: formatErrorMessage(error),
      });
    } finally {
      setActiveFavoriteId(null);
    }
  };

  useEffect((): void => {
    resetLauncherView();
    void loadHistory("loading", "加载失败");
  }, []);

  useEffect((): (() => void) => {
    let isDisposed = false;
    const cleanupList: Array<() => void> = [];

    void Promise.all([
      listen(HISTORY_UPDATED_EVENT, () => {
        void loadHistory("refreshing", "刷新失败");
      }),
      listen(LAUNCHER_SHOWN_EVENT, () => {
        resetLauncherView();
      }),
    ])
      .then((cleanup) => {
        if (isDisposed) {
          cleanup.forEach((dispose) => dispose());
          return;
        }

        cleanupList.push(...cleanup);
      })
      .catch((error) => {
        if (isDisposed) {
          return;
        }

        setErrorState({
          title: "刷新失败",
          message: formatErrorMessage(error),
        });
      });

    return () => {
      isDisposed = true;
      cleanupList.forEach((cleanup) => cleanup());
    };
  }, []);

  useEffect((): (() => void) => {
    const handleWindowFocus = (): void => {
      resetLauncherView();
    };

    window.addEventListener("focus", handleWindowFocus);

    return () => {
      window.removeEventListener("focus", handleWindowFocus);
    };
  }, []);

  const filteredHistory = getVisibleHistoryItems(history, query, activeGroup, activeSort);
  const selectedItem =
    filteredHistory.find((item) => item.id === selectedId) ?? filteredHistory[0] ?? null;
  const isLoading = historyStatus === "loading";
  const isRefreshing = historyStatus === "refreshing";
  const isMutating =
    activeDeleteId !== null ||
    activeCopyId !== null ||
    activePinId !== null ||
    activeFavoriteId !== null ||
    isClearing;
  const hasClearableItems = history.some((item) => !item.isFavorite);

  useEffect((): void => {
    latestHistoryRef.current = history;
  }, [history]);

  useEffect((): void => {
    latestQueryRef.current = query;
  }, [query]);

  useEffect((): void => {
    latestGroupRef.current = activeGroup;
  }, [activeGroup]);

  useEffect((): void => {
    latestSortRef.current = activeSort;
  }, [activeSort]);

  useEffect((): void => {
    latestSelectedIdRef.current = selectedId;
  }, [selectedId]);

  useEffect((): void => {
    if (!selectedItem) {
      if (selectedId !== null) {
        setSelectedId(null);
      }
      return;
    }

    if (selectedItem.id !== selectedId) {
      setSelectedId(selectedItem.id);
    }
  }, [selectedId, selectedItem]);

  useEffect((): void => {
    if (!shouldResetSelectionRef.current) {
      return;
    }

    const nextSelectedId = filteredHistory[0]?.id ?? null;

    shouldResetSelectionRef.current = false;
    setSelectedId(nextSelectedId);

    window.requestAnimationFrame(() => {
      launcherListRef.current?.scrollTo({ top: 0, behavior: "auto" });
    });
  }, [filteredHistory]);

  useEffect((): void => {
    if (!selectedItem || !launcherListRef.current) {
      return;
    }

    window.requestAnimationFrame(() => {
      const selectedElement = launcherListRef.current?.querySelector<HTMLElement>(
        `[data-history-id="${selectedItem.id}"]`,
      );

      selectedElement?.scrollIntoView({
        block: "nearest",
        inline: "nearest",
      });
    });
  }, [selectedItem]);

  useEffect((): (() => void) => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.target instanceof HTMLSelectElement) {
        return;
      }

      if (event.key === "Escape") {
        event.preventDefault();
        void hideLauncherWindow();
        return;
      }

      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();

        const offset = event.key === "ArrowDown" ? 1 : -1;

        if (activeArrowKeyRef.current !== event.key) {
          clearArrowKeyRepeat();
          activeArrowKeyRef.current = event.key;
          moveSelectionByOffset(offset);
          keyRepeatTimeoutRef.current = window.setTimeout(() => {
            moveSelectionByOffset(offset);
            keyRepeatIntervalRef.current = window.setInterval(() => {
              moveSelectionByOffset(offset);
            }, KEY_REPEAT_INTERVAL_MS);
          }, KEY_REPEAT_INITIAL_DELAY_MS);
        }

        return;
      }

      const visibleItems = getVisibleHistoryItems(
        latestHistoryRef.current,
        latestQueryRef.current,
        latestGroupRef.current,
        latestSortRef.current,
      );

      if (visibleItems.length === 0) {
        return;
      }

      if (event.key === "Enter" && !isMutating) {
        const currentSelectedItem =
          latestSelectedIdRef.current === null
            ? null
            : visibleItems.find((item) => item.id === latestSelectedIdRef.current) ?? null;

        if (!currentSelectedItem || document.activeElement === previewRef.current) {
          return;
        }

        event.preventDefault();
        void pasteHistoryItem(currentSelectedItem);
      }
    };

    const handleKeyUp = (event: KeyboardEvent): void => {
      if (event.key === activeArrowKeyRef.current) {
        clearArrowKeyRepeat();
      }
    };

    const handleWindowBlur = (): void => {
      clearArrowKeyRepeat();
    };

    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleWindowBlur);

    return () => {
      clearArrowKeyRepeat();
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleWindowBlur);
    };
  }, [isMutating]);

  const handleSearchKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>): void => {
    if (event.key === "ArrowDown" && filteredHistory.length > 0 && selectedId === null) {
      event.preventDefault();
      setSelectedId(filteredHistory[0]?.id ?? null);
    }
  };

  const previewRef = useRef<HTMLPreElement | null>(null);
  const customCopy = async () => {
    if (previewRef.current) {
      let content = previewRef.current.innerText
      // 去除最后的换行
      content = content.trimEnd()
      await invoke("paste_custom_history", { content });
    }
  }

  return (
    <main className="launcher-shell">
      <section className="launcher">
        <label className="search-box" htmlFor="history-search">
          <span className="search-prefix" aria-hidden="true">
            <SearchIcon />
          </span>
          <input
            id="history-search"
            ref={searchInputRef}
            className="search-input"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={handleSearchKeyDown}
            placeholder="搜索文本、文件路径或图片路径"
            autoComplete="off"
            spellCheck={false}
          />
        </label>
        {errorState ? (
          <section className="error-banner" role="alert">
            <strong>{errorState.title}</strong>
            <p>{errorState.message}</p>
          </section>
        ) : null}

        <section className="launcher-content">
          <div className="launcher-panel launcher-panel-list">
            <div className="launcher-list-controls">
              <div className="launcher-group-tabs" role="tablist" aria-label="历史记录类型">
                {LIST_GROUP_OPTIONS.map((option) => {
                  const isActive = option.key === activeGroup;

                  return (
                    <button
                      key={option.key}
                      type="button"
                      className={`launcher-group-tab${isActive ? " is-active" : ""}`}
                      onClick={() => setActiveGroup(option.key)}
                    >
                      <span>{option.label}</span>
                      <span className="launcher-group-count">{getListGroupCount(history, option.key)}</span>
                    </button>
                  );
                })}
              </div>
              <div className="launcher-control-actions">
                <p className="launcher-inline-status">{getVisibleItemCountText(filteredHistory, isRefreshing)}</p>
                <label className="launcher-sort-box" htmlFor="launcher-sort-select">
                  <select
                    id="launcher-sort-select"
                    className="launcher-sort-select"
                    value={activeSort}
                    onChange={(event) => setActiveSort(event.target.value as TListSortKey)}
                  >
                    {Object.entries(SORT_OPTION_LABELS).map(([key, label]) => (
                      <option key={key} value={key}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  className="launcher-clean-all"
                  onClick={() => void handleClearHistory()}
                  disabled={isMutating || !hasClearableItems}
                >
                  {isClearing ? "清空中..." : "清空"}
                </button>
              </div>
            </div>

            {isLoading ? (
              <section className="empty-state">
                <h1>正在读取剪贴板历史</h1>
                <p>本地历史记录加载完成后会立即显示在这里。</p>
              </section>
            ) : null}

            {!isLoading && !errorState && history.length === 0 ? (
              <section className="empty-state">
                <h1>还没有历史记录</h1>
                <p>复制文本、图片或文件路径后，结果会自动出现在这里。</p>
              </section>
            ) : null}

            {!isLoading && history.length > 0 && filteredHistory.length === 0 ? (
              <section className="empty-state empty-state-compact">
                <h1>没有匹配项</h1>
                <p>尝试搜索文本内容、文件路径，或图片文件路径。</p>
              </section>
            ) : null}

            {!isLoading && filteredHistory.length > 0 ? (
              <section ref={launcherListRef} className="launcher-list" aria-live="polite">
                {filteredHistory.map((item) => {
                  const isSelected = item.id === selectedItem?.id;
                  const isDeleting = activeDeleteId === item.id;
                  const isCopying = activeCopyId === item.id;
                  const isPinning = activePinId === item.id;
                  const isFavoriting = activeFavoriteId === item.id;

                  return (
                    <article
                      key={item.id}
                      data-history-id={item.id}
                      className={`launcher-item${isSelected ? " is-selected" : ""}`}
                      onClick={() => setSelectedId(item.id)}
                      onDoubleClick={() => void copyHistoryItem(item)}
                    >
                      {item.contentType === "image" && item.imagePath ? null : (
                        <div className="launcher-item-badge">{getItemBadgeText(item)}</div>
                      )}
                      <div className="launcher-item-content">
                        {renderListPreview(item)}
                      </div>
                      <span className="history-time launcher-item-time">{formatTimestamp(item.updatedAt)}</span>
                      <div className="launcher-item-actions">
                        {/* <button
                          type="button"
                          className={`item-icon-button${item.isPinned ? " is-active" : ""}`}
                          onClick={(event) => {
                            event.stopPropagation();
                            void handleTogglePin(item.id);
                          }}
                          disabled={isMutating}
                          aria-label={item.isPinned ? "取消置顶" : "置顶"}
                          title={isPinning ? "处理中" : item.isPinned ? "取消置顶" : "置顶"}
                        >
                          <PinIcon filled={item.isPinned} />
                        </button> */}
                        <button
                          type="button"
                          className={`item-icon-button${item.isFavorite ? " is-active" : ""}`}
                          onClick={(event) => {
                            event.stopPropagation();
                            void handleToggleFavorite(item.id);
                          }}
                          disabled={isMutating}
                          aria-label={item.isFavorite ? "取消收藏" : "收藏"}
                          title={isFavoriting ? "处理中" : item.isFavorite ? "取消收藏" : "收藏"}
                        >
                          <StarIcon filled={item.isFavorite} />
                        </button>
                        {
                          item.isFavorite ? null : <button
                          type="button"
                          className="item-icon-button item-delete-button"
                          onClick={(event) => {
                            event.stopPropagation();
                            void handleDelete(item.id);
                          }}
                          disabled={isMutating}
                          aria-label={isDeleting ? "删除中" : isCopying ? "复制中" : "删除"}
                          title={isDeleting ? "删除中" : isCopying ? "复制中" : "删除"}
                        >
                          <TrashIcon />
                        </button>
                        }
                      </div>
                    </article>
                  );
                })}
              </section>
            ) : null}
          </div>

          <aside className="launcher-panel launcher-panel-preview">
            {selectedItem ? (
              <>
                <div className="preview-header">
                  <span className="history-type-pill">{getItemTypeLabel(selectedItem)}</span>
                  {selectedItem.isPinned ? <span className="history-flag-pill">已置顶</span> : null}
                  {selectedItem.isFavorite ? <span className="history-flag-pill">已收藏</span> : null}
                  <span className="history-time">{formatTimestamp(selectedItem.updatedAt)}</span>
                </div>
                <div className="preview-card">
                  <div className="preview-content">
                    {renderPreviewContent(selectedItem, previewRef)}
                  </div>
                  <div className="preview-footer">
                    <button onClick={customCopy} type="button" className="preview-footer-button">复制</button>
                  </div>
                </div>
              </>
            ) : (
              <section className="empty-state empty-state-preview">
                <h1>暂无预览内容</h1>
                <p>左侧选中一条历史记录后，这里会显示原文。</p>
              </section>
            )}
          </aside>
        </section>
      </section>
    </main>
  );
};

export default App;
