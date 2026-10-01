import { useLogicalNavigation } from "../logical-navigation";
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Accordion, Alert, Button, Card, Spinner } from "@heroui/react";
import { LuArrowLeft, LuArrowRight, LuArrowUpRight, LuImage, LuMegaphone } from "react-icons/lu";
import { useAppContent } from "./context";
import { contentTime, openPublicUrl } from "./external";
import type { Announcement } from "./types";
import { EMPTY_ELECTRICITY_STATUS, useElectricityMonitor } from "../electricity-monitor";

type LocalAnnouncement = {
  kind: "electricity-low";
  title: string;
  url: "/campus-payments/recharge";
};
type AnnouncementUnit = Announcement | LocalAnnouncement;

export function AnnouncementImage({ url, title }: { url: string; title: string }) {
  const [image, setImage] = useState<{ url: string; src?: string; failed?: boolean }>();
  const request = useRef<{ url: string; promise: Promise<string> } | undefined>(undefined);
  useEffect(() => {
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
          onError={() => setImage({ url, failed: true })}
        />
      ) : (
        <span
          className="announcement-image-placeholder"
          role="img"
          aria-label={current?.failed ? title + "头图加载失败" : "正在加载" + title + "头图"}
        >
          {current?.failed ? <LuImage aria-hidden="true" /> : <Spinner size="sm" />}
          <span>{current?.failed ? "头图暂不可用" : "正在加载头图"}</span>
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
      <Card className="announcement-card">
        <Button
          className="announcement-trigger announcement-link"
          variant="tertiary"
          aria-label={unit.title}
          onPress={() => {
            void navigation.push(unit.url);
          }}
        >
          <span className="announcement-image-placeholder" aria-hidden="true">
            <LuMegaphone />
          </span>
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
      <Card className="announcement-card">
        <Accordion hideSeparator>
          <Accordion.Item>
            <Accordion.Heading>
              <Accordion.Trigger className="announcement-trigger">
                <AnnouncementImage url={unit.pic} title={unit.title} />
                <span className="announcement-title-row">
                  <span>{unit.title}</span>
                  <Accordion.Indicator />
                </span>
              </Accordion.Trigger>
            </Accordion.Heading>
            <Accordion.Panel>
              <Accordion.Body className="announcement-body content-plaintext">
                {unit.desc}
              </Accordion.Body>
            </Accordion.Panel>
          </Accordion.Item>
        </Accordion>
      </Card>
    );
  }
  return (
    <Card className="announcement-card">
      <Button
        className="announcement-trigger announcement-link"
        variant="tertiary"
        aria-label={unit.title + "（在系统浏览器打开）"}
        onPress={() => {
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
  const feed = snapshot?.announcements;
  const units = feed?.data?.announcements;
  const localUnit: LocalAnnouncement | undefined =
    electricity.low && electricity.remaining != null
      ? {
          kind: "electricity-low",
          title: `当前剩余电量：${electricity.remaining.toFixed(2)} 度${electricity.warningReason ? ` · ${electricity.warningReason}` : ""}`,
          url: "/campus-payments/recharge",
        }
      : undefined;
  const displayUnits: AnnouncementUnit[] = localUnit
    ? [localUnit, ...(units ?? [])]
    : (units ?? []);
  const unavailable = snapshot?.configuration.status !== "ready";
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
            <Spinner size="sm" /> 正在刷新
          </span>
        )}
      </div>
      {error && (
        <p className="content-muted" aria-live="polite">
          {error}
        </p>
      )}
      {feed?.error && (
        <Alert status="warning">
          <Alert.Indicator />
          <Alert.Content>
            <Alert.Title>本次公告获取失败</Alert.Title>
            <Alert.Description>
              {feed.error.message}
              {feed.fromCache &&
                " 正在显示上次成功获取的公告（" + contentTime(feed.lastSuccessAt) + "）。"}
            </Alert.Description>
          </Alert.Content>
        </Alert>
      )}
      {displayUnits.length ? (
        <div
          className="announcement-carousel"
          data-testid="announcement-carousel"
          role="region"
          aria-roledescription="carousel"
          aria-label="校园公告轮播"
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
                  variant="secondary"
                  aria-label="上一条公告"
                  onPress={showPrevious}
                >
                  <LuArrowLeft aria-hidden="true" />
                </Button>
                <Button
                  className="announcement-carousel-control announcement-carousel-control-next"
                  variant="secondary"
                  aria-label="下一条公告"
                  onPress={showNext}
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
        </div>
      ) : (
        <Card className="announcement-empty">
          <Card.Content>
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
          </Card.Content>
        </Card>
      )}
    </section>
  );
}
