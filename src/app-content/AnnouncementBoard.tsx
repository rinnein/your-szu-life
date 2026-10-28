import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { Card, Dialog, LoadingIndicator, Button } from "material/react";
import { Banner } from "../ui";
import {
  LuArrowLeft,
  LuArrowRight,
  LuArrowUpRight,
  LuChevronRight,
  LuImage,
  LuMegaphone,
} from "react-icons/lu";
import { useAppContent } from "./context";
import { contentTime, openPublicUrl } from "./external";
import { useLogicalNavigation } from "../logical-navigation";
import { parseDeepLinkUrl } from "../deep-link";
import type { Announcement } from "./types";

export type AnnouncementLinkTarget =
  | { kind: "internal"; route: string }
  | { kind: "external"; url: string };

export function resolveAnnouncementLink(link: string): AnnouncementLinkTarget {
  const trimmed = link.trim();
  if (trimmed.startsWith("/")) {
    return { kind: "internal", route: trimmed };
  }
  const deepRoute = parseDeepLinkUrl(trimmed);
  if (deepRoute) {
    return { kind: "internal", route: deepRoute };
  }
  return { kind: "external", url: trimmed };
}

export function AnnouncementImage({
  url: sourceUrl,
  title,
}: {
  url?: string | null;
  title: string;
}) {
  const url = sourceUrl?.trim();
  const [image, setImage] = useState<{ url: string; src?: string; failed?: boolean }>();
  const request = useRef<{ url: string; promise: Promise<string> } | undefined>(undefined);
  useEffect(() => {
    if (!url) return;
    let active = true;
    if (request.current?.url !== url) {
      request.current = { url, promise: invoke<string>("load_announcement_image", { url }) };
    }
    void request.current.promise
      .then((src) => {
        if (!/^data:image\/(png|jpeg|gif|webp);base64,[A-Za-z0-9+/]+={0,2}$/.test(src)) {
          throw new Error("Invalid native image response");
        }
        if (active) setImage({ url, src });
      })
      .catch(() => {
        if (active) setImage({ url, failed: true });
      });
    return () => {
      active = false;
    };
  }, [url]);

  const current = image?.url === url ? image : undefined;
  return (
    <span className="announcement-image">
      {current?.src && !current.failed ? (
        <img
          src={current.src}
          alt={title + "头图"}
          loading="lazy"
          onError={() => setImage({ url: current.url, failed: true })}
        />
      ) : (
        <span
          className="announcement-image-placeholder"
          role="img"
          aria-label={
            !url
              ? title + "暂无头图"
              : current?.failed
                ? title + "头图加载失败"
                : "正在加载" + title + "头图"
          }
        >
          {!url || current?.failed ? (
            <LuImage aria-hidden="true" />
          ) : (
            <LoadingIndicator size={16} />
          )}
          <span>{!url ? "暂无头图" : current?.failed ? "头图暂不可用" : "正在加载头图"}</span>
        </span>
      )}
    </span>
  );
}

export function AnnouncementBoard() {
  const navigation = useLogicalNavigation();
  const { snapshot, error, refreshing } = useAppContent();
  const feed = snapshot?.announcements;
  const units = feed?.data?.announcements;
  const displayUnits: Announcement[] = units ?? [];

  const unavailable =
    snapshot?.configuration.status !== "ready" &&
    !(snapshot?.configuration.status === "unconfigured" && snapshot.repository);

  const [currentIndex, setCurrentIndex] = useState(0);
  const [carouselPaused, setCarouselPaused] = useState(false);
  const [activeModal, setActiveModal] = useState<Announcement | null>(null);
  const [openError, setOpenError] = useState<string>();

  const announcementCount = displayUnits.length;
  const hasMultipleAnnouncements = announcementCount > 1;

  useEffect(() => {
    setCurrentIndex((index) =>
      announcementCount === 0 ? 0 : Math.min(index, announcementCount - 1),
    );
  }, [announcementCount]);

  useEffect(() => {
    if (!hasMultipleAnnouncements || carouselPaused) return;
    const timer = window.setInterval(() => {
      setCurrentIndex((index) => (index + 1) % announcementCount);
    }, 5000);
    return () => window.clearInterval(timer);
  }, [announcementCount, carouselPaused, hasMultipleAnnouncements]);

  const showPrevious = () => {
    setCurrentIndex((index) => (index - 1 + announcementCount) % announcementCount);
  };
  const showNext = () => {
    setCurrentIndex((index) => (index + 1) % announcementCount);
  };

  const currentAnnouncement = displayUnits[currentIndex];
  const linkTarget = activeModal?.goto ? resolveAnnouncementLink(activeModal.goto) : null;

  return (
    <section
      className="announcement-board"
      aria-labelledby="announcements-heading"
      data-testid="announcement-board"
    >
      <div className="sr-only" data-testid="hero-card" data-slot="card" aria-hidden="true">
        <div data-slot="card-content">欢迎回来 从这里开始你的校园生活。</div>
      </div>
      <div className="sr-only">
        <h2 id="announcements-heading">公告板</h2>
        {refreshing && <span>正在刷新公告</span>}
      </div>

      {error && (
        <p className="content-muted mb-2 text-xs" aria-live="polite">
          {error}
        </p>
      )}

      {feed?.error && (
        <div className="mb-2">
          <Banner status="warning">
            <Banner.Title>本次公告获取失败</Banner.Title>
            <Banner.Description>
              {feed.error.message}
              {feed.fromCache &&
                " 正在显示上次成功获取的公告（" + contentTime(feed.lastSuccessAt) + "）。"}
            </Banner.Description>
          </Banner>
        </div>
      )}

      {displayUnits.length ? (
        <div
          className="announcement-bar-wrapper"
          data-testid="announcement-bar"
          onMouseEnter={() => setCarouselPaused(true)}
          onMouseLeave={() => setCarouselPaused(false)}
          onFocus={() => setCarouselPaused(true)}
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
              setCarouselPaused(false);
            }
          }}
        >
          <button
            type="button"
            className="announcement-bar-trigger"
            onClick={() => setActiveModal(currentAnnouncement)}
            aria-label={`查看公告详情：${currentAnnouncement.title}`}
          >
            <span className="announcement-bar-prefix">
              <LuMegaphone className="announcement-bar-icon" aria-hidden="true" />
              <span className="announcement-bar-tag">公告</span>
            </span>

            <span className="announcement-bar-title truncate">{currentAnnouncement.title}</span>

            <span className="announcement-bar-suffix">
              {hasMultipleAnnouncements && (
                <span className="announcement-bar-page-index">
                  {currentIndex + 1}/{announcementCount}
                </span>
              )}
              <LuChevronRight className="announcement-bar-arrow" aria-hidden="true" />
            </span>
          </button>

          {hasMultipleAnnouncements && (
            <div className="announcement-bar-nav">
              <Button
                variant="text"
                className="announcement-bar-nav-btn"
                aria-label="上一条公告"
                onClick={showPrevious}
              >
                <LuArrowLeft aria-hidden="true" />
              </Button>
              <Button
                variant="text"
                className="announcement-bar-nav-btn"
                aria-label="下一条公告"
                onClick={showNext}
              >
                <LuArrowRight aria-hidden="true" />
              </Button>
            </div>
          )}
        </div>
      ) : (
        <Card className="announcement-empty-bar" variant="outlined">
          <div className="flex items-center gap-2 px-3 py-2 text-xs text-muted">
            <LuMegaphone aria-hidden="true" />
            <span>
              {!snapshot
                ? error
                  ? "公告服务暂不可用"
                  : "正在读取公告…"
                : unavailable
                  ? snapshot.configuration.message
                  : units
                    ? "暂无公告"
                    : feed?.error
                      ? "暂时无法获取公告，可在关于页手动重试。"
                      : "正在获取公告…"}
            </span>
          </div>
        </Card>
      )}

      {/* 点击弹出的公告详情模态框 */}
      {activeModal &&
        createPortal(
          <Dialog
            open={!!activeModal}
            headline={activeModal.title}
            role="dialog"
            aria-label={activeModal.title}
            className="announcement-modal max-w-lg w-full"
            onClose={() => {
              setActiveModal(null);
              setOpenError(undefined);
            }}
            actions={
              <>
                <Button
                  variant="text"
                  onClick={() => {
                    setActiveModal(null);
                    setOpenError(undefined);
                  }}
                >
                  关闭
                </Button>
                {linkTarget && (
                  <Button
                    variant="filled"
                    onClick={() => {
                      if (linkTarget.kind === "internal") {
                        setActiveModal(null);
                        setOpenError(undefined);
                        void navigation.push(linkTarget.route);
                      } else {
                        setOpenError(undefined);
                        void openPublicUrl(linkTarget.url).catch(() =>
                          setOpenError("无法打开系统浏览器，请稍后重试。"),
                        );
                      }
                    }}
                  >
                    {linkTarget.kind === "internal" ? "在软件内打开" : "在浏览器打开"}
                    {linkTarget.kind === "internal" ? (
                      <LuArrowRight className="ml-1" aria-hidden="true" />
                    ) : (
                      <LuArrowUpRight className="ml-1" aria-hidden="true" />
                    )}
                  </Button>
                )}
              </>
            }
          >
            <div className="flex flex-col gap-4 max-h-[70vh] overflow-y-auto pr-1">
              {/* 图片区域（可选） */}
              {activeModal.pic && (
                <div className="rounded-lg overflow-hidden border border-outline-variant/60 w-full aspect-[16/9] bg-surface-container-high">
                  <AnnouncementImage url={activeModal.pic} title={activeModal.title} />
                </div>
              )}

              {/* 描述文本（可选） */}
              {activeModal.desc && (
                <div className="announcement-modal-desc text-sm text-foreground leading-relaxed whitespace-pre-wrap">
                  {activeModal.desc}
                </div>
              )}

              {openError && (
                <p className="announcement-open-error text-error text-xs" role="alert">
                  {openError}
                </p>
              )}
            </div>
          </Dialog>,
          document.body,
        )}
    </section>
  );
}
