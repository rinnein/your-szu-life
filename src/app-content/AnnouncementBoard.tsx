import { useCallback, useEffect, useState } from "react";
import { Card, LoadingIndicator, Button } from "material/react";
import { Banner } from "../ui";
import { useRemoteImage } from "../resources/remote";
import { LuArrowLeft, LuArrowRight, LuChevronRight, LuImage, LuMegaphone } from "react-icons/lu";
import { useAppContent } from "./context";
import { contentTime } from "./external";
import { parseDeepLinkUrl } from "../deep-link";
import { AnnouncementModal } from "./AnnouncementModal";
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
  const url = sourceUrl?.trim() ?? "";
  const { src, failed } = useRemoteImage(url || undefined, "announcement");
  const [broken, setBroken] = useState(false);
  useEffect(() => {
    setBroken(false);
  }, [url]);

  const failedNow = Boolean(url) && (failed || broken);
  return (
    <span className="announcement-image">
      {url && src && !failedNow ? (
        <img src={src} alt={title + "头图"} loading="lazy" onError={() => setBroken(true)} />
      ) : (
        <span
          className="announcement-image-placeholder"
          role="img"
          aria-label={
            !url
              ? title + "暂无头图"
              : failedNow
                ? title + "头图加载失败"
                : "正在加载" + title + "头图"
          }
        >
          {!url || failedNow ? <LuImage aria-hidden="true" /> : <LoadingIndicator size={16} />}
          <span>{!url ? "暂无头图" : failedNow ? "头图暂不可用" : "正在加载头图"}</span>
        </span>
      )}
    </span>
  );
}

export function AnnouncementBoard() {
  const { snapshot, error, refreshing } = useAppContent();
  const feed = snapshot?.announcements;
  const units = feed?.data?.announcements;
  const displayUnits: Announcement[] = units ?? [];

  const unavailable =
    snapshot?.configuration.status !== "ready" &&
    !(snapshot?.configuration.status === "unconfigured" && snapshot.repository);

  const [currentIndex, setCurrentIndex] = useState(0);
  const [carouselPaused, setCarouselPaused] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);

  const announcementCount = displayUnits.length;
  const hasMultipleAnnouncements = announcementCount > 1;

  useEffect(() => {
    setCurrentIndex((index) =>
      announcementCount === 0 ? 0 : Math.min(index, announcementCount - 1),
    );
    // 公告列表变空（例如刷新后已经撤下）时不要再把详情模态框留在屏幕上。
    setModalOpen((open) => open && announcementCount > 0);
  }, [announcementCount]);

  useEffect(() => {
    // 详情模态框打开时暂停自动轮播，否则内容会在阅读中途自己换掉。
    if (!hasMultipleAnnouncements || carouselPaused || modalOpen) return;
    const timer = window.setInterval(() => {
      setCurrentIndex((index) => (index + 1) % announcementCount);
    }, 5000);
    return () => window.clearInterval(timer);
  }, [announcementCount, carouselPaused, hasMultipleAnnouncements, modalOpen]);

  const showPrevious = useCallback(() => {
    setCurrentIndex((index) =>
      announcementCount === 0 ? 0 : (index - 1 + announcementCount) % announcementCount,
    );
  }, [announcementCount]);
  const showNext = useCallback(() => {
    setCurrentIndex((index) => (announcementCount === 0 ? 0 : (index + 1) % announcementCount));
  }, [announcementCount]);

  const currentAnnouncement = displayUnits[currentIndex];

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
            onClick={() => setModalOpen(true)}
            aria-label={`查看公告详情：${currentAnnouncement.title}`}
          >
            <span className="announcement-bar-prefix">
              <LuMegaphone className="announcement-bar-icon" aria-hidden="true" />
              <span className="announcement-bar-tag">公告</span>
            </span>

            <span className="announcement-bar-title truncate">{currentAnnouncement.title}</span>

            <span className="announcement-bar-suffix">
              {hasMultipleAnnouncements && (
                <span className="announcement-nav-index">
                  {currentIndex + 1}/{announcementCount}
                </span>
              )}
              <LuChevronRight className="announcement-bar-arrow" aria-hidden="true" />
            </span>
          </button>

          {hasMultipleAnnouncements && (
            <div className="announcement-nav">
              <Button
                variant="text"
                className="announcement-nav-btn"
                aria-label="上一条公告"
                onClick={showPrevious}
              >
                <LuArrowLeft aria-hidden="true" />
              </Button>
              <Button
                variant="text"
                className="announcement-nav-btn"
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

      {/* 点击弹出的公告详情模态框：与横条共用 currentIndex，页脚可翻看相邻公告 */}
      {modalOpen && currentAnnouncement && (
        <AnnouncementModal
          announcement={currentAnnouncement}
          index={currentIndex}
          total={announcementCount}
          onPrevious={showPrevious}
          onNext={showNext}
          onClose={() => setModalOpen(false)}
        />
      )}
    </section>
  );
}
