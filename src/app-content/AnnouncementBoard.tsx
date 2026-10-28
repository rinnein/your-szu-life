import { useLogicalNavigation } from "../logical-navigation";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Card, Carousel, LoadingIndicator, Button } from "material/react";
import { Banner, Disclosure } from "../ui";
import { LuArrowLeft, LuArrowRight, LuArrowUpRight, LuImage, LuMegaphone } from "react-icons/lu";
import { useAppContent } from "./context";
import { contentTime, openPublicUrl } from "./external";
import type { Announcement } from "./types";
import { EMPTY_ELECTRICITY_STATUS, useElectricityMonitor } from "../electricity-monitor";
import { useActiveOjContests } from "../oj/active-contests";

type LocalAnnouncement =
  | {
      kind: "electricity-low";
      title: string;
      url: "/life/campus-payments/recharge";
    }
  | {
      kind: "oj-active";
      title: string;
      url: string;
    };
type AnnouncementUnit = Announcement | LocalAnnouncement;

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
    // Use the restricted native channel for HTTPS as well: no WebView auth cookies,
    // no CORS dependency, and identical validation/caching on desktop and Android.
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

function AnnouncementCard({ unit }: { unit: AnnouncementUnit }) {
  const navigation = useLogicalNavigation();
  const [error, setError] = useState<string>();
  if ("kind" in unit) {
    return (
      <Card className="announcement-card" variant="outlined">
        <Button
          className="announcement-trigger announcement-link"
          variant="text"
          aria-label={unit.title}
          onClick={() => {
            void navigation.push(unit.url);
          }}
        >
          <AnnouncementImage title={unit.title} />
          <span className="announcement-title-row">
            <span>{unit.title}</span>
            <LuArrowUpRight aria-hidden="true" />
          </span>
        </Button>
      </Card>
    );
  }
  if (unit.desc !== undefined) {
    return (
      <Card className="announcement-card" variant="outlined">
        <Disclosure
          title={
            <span className="announcement-trigger">
              <AnnouncementImage url={unit.pic} title={unit.title} />
              <span className="announcement-title-row">
                <span>{unit.title}</span>
              </span>
            </span>
          }
        >
          <div className="announcement-body content-plaintext">{unit.desc}</div>
        </Disclosure>
      </Card>
    );
  }
  return (
    <Card className="announcement-card" variant="outlined">
      <Button
        className="announcement-trigger announcement-link"
        variant="text"
        aria-label={unit.title + "（在系统浏览器打开）"}
        onClick={() => {
          setError(undefined);
          void openPublicUrl(unit.goto).catch(() => setError("无法打开系统浏览器，请稍后重试。"));
        }}
      >
        <AnnouncementImage url={unit.pic} title={unit.title} />
        <span className="announcement-title-row">
          <span>{unit.title}</span>
          <LuArrowUpRight aria-hidden="true" />
        </span>
      </Button>
      {error && (
        <p className="announcement-open-error" role="alert">
          {error}
        </p>
      )}
    </Card>
  );
}

export function AnnouncementBoard() {
  const { snapshot, error, refreshing } = useAppContent();
  const electricity = useElectricityMonitor() ?? EMPTY_ELECTRICITY_STATUS;
  const activeOjContests = useActiveOjContests();
  const feed = snapshot?.announcements;
  const units = feed?.data?.announcements;

  const localUnits: LocalAnnouncement[] = [];
  if (electricity.low && electricity.remaining != null) {
    localUnits.push({
      kind: "electricity-low",
      title: `当前剩余电量：${electricity.remaining.toFixed(2)} 度${electricity.warningReason ? ` · ${electricity.warningReason}` : ""}`,
      url: "/life/campus-payments/recharge",
    });
  }

  for (const contest of activeOjContests) {
    localUnits.push({
      kind: "oj-active",
      title: `进行中测验：${contest.title}`,
      url: contest.url,
    });
  }

  const displayUnits: AnnouncementUnit[] = [...localUnits, ...(units ?? [])];
  const unavailable =
    snapshot?.configuration.status !== "ready" &&
    !(snapshot?.configuration.status === "unconfigured" && snapshot.repository);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [carouselPaused, setCarouselPaused] = useState(false);
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

  return (
    <section
      className="announcement-board"
      aria-labelledby="announcements-heading"
      data-testid="announcement-board"
    >
      <div className="sr-only" data-testid="hero-card" data-slot="card" aria-hidden="true">
        <div data-slot="card-content">欢迎回来 从这里开始你的校园生活。</div>
      </div>
      <div className="content-section-header">
        <h2 id="announcements-heading">
          <LuMegaphone aria-hidden="true" /> 公告板
        </h2>
        {refreshing && (
          <span className="content-muted" role="status">
            <LoadingIndicator size={16} /> 正在刷新
          </span>
        )}
      </div>
      {error && (
        <p className="content-muted" aria-live="polite">
          {error}
        </p>
      )}
      {feed?.error && (
        <Banner status="warning">
          <Banner.Title>本次公告获取失败</Banner.Title>
          <Banner.Description>
            {feed.error.message}
            {feed.fromCache &&
              " 正在显示上次成功获取的公告（" + contentTime(feed.lastSuccessAt) + "）。"}
          </Banner.Description>
        </Banner>
      )}
      {displayUnits.length ? (
        <Carousel
          className="announcement-carousel"
          data-testid="announcement-carousel"
          role="region"
          aria-roledescription="carousel"
          aria-label="校园公告轮播"
          index={currentIndex}
          onChange={(e: any) => setCurrentIndex(e.detail?.value ?? e.target?.value ?? 0)}
          onMouseEnter={() => setCarouselPaused(true)}
          onMouseLeave={() => setCarouselPaused(false)}
          onFocus={() => setCarouselPaused(true)}
          onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
              setCarouselPaused(false);
            }
          }}
        >
          <div className="announcement-viewport">
            <div
              className="announcement-slide"
              data-testid="announcement-slide"
              role="group"
              aria-roledescription="slide"
              aria-label={`第 ${currentIndex + 1} 条公告，共 ${announcementCount} 条`}
            >
              <AnnouncementCard unit={displayUnits[currentIndex]} />
            </div>
            {hasMultipleAnnouncements && (
              <>
                <Button
                  className="announcement-carousel-control announcement-carousel-control-previous"
                  variant="tonal"
                  aria-label="上一条公告"
                  onClick={showPrevious}
                >
                  <LuArrowLeft aria-hidden="true" />
                </Button>
                <Button
                  className="announcement-carousel-control announcement-carousel-control-next"
                  variant="tonal"
                  aria-label="下一条公告"
                  onClick={showNext}
                >
                  <LuArrowRight aria-hidden="true" />
                </Button>
              </>
            )}
          </div>
          {hasMultipleAnnouncements && (
            <div className="announcement-carousel-indicators" aria-label="公告页码">
              {displayUnits.map((unit, index) => (
                <button
                  key={index + ":" + unit.title}
                  className="announcement-carousel-indicator"
                  type="button"
                  aria-label={`查看第 ${index + 1} 条公告`}
                  aria-current={index === currentIndex ? "true" : undefined}
                  onClick={() => setCurrentIndex(index)}
                />
              ))}
            </div>
          )}
        </Carousel>
      ) : (
        <Card className="announcement-empty" variant="outlined">
          <div className="p-6 text-center">
            <p>
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
            </p>
          </div>
        </Card>
      )}
    </section>
  );
}
