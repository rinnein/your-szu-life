import { useState, type ReactNode } from "react";
import { Button, Card } from "@heroui/react";
import { QRCodeSVG } from "qrcode.react";
import { LuCopy } from "react-icons/lu";
import { copyText } from "./clipboard";
import { WECHAT_SERVICES } from "./campus-payments";

export function WeChatServiceCard({
  service,
  icon,
}: {
  service: (typeof WECHAT_SERVICES)[number]["id"];
  icon: ReactNode;
}) {
  const data = WECHAT_SERVICES.find((x) => x.id === service)!;
  const [copying, setCopying] = useState(false);
  const [message, setMessage] = useState<string>();
  const copy = async () => {
    setCopying(true);
    setMessage(undefined);
    setMessage(
      (await copyText(data.url))
        ? "链接已复制，请在微信内粘贴打开。"
        : "无法自动复制，请长按或选择下方完整链接手动复制。",
    );
    setCopying(false);
  };
  return (
    <Card className="payments-card" aria-labelledby={data.id + "-heading"}>
      <Card.Header>
        <Card.Title id={data.id + "-heading"} className="payments-card-title">
          {icon}
          {data.title}
        </Card.Title>
        <Card.Description>{data.description}</Card.Description>
      </Card.Header>
      <Card.Content className="payments-wechat-content">
        <p className="payments-instruction">复制链接后在微信内打开，或使用微信扫码</p>
        <Button
          variant="secondary"
          aria-label={"复制" + data.title + "链接"}
          isDisabled={copying}
          onPress={() => void copy()}
        >
          <LuCopy aria-hidden="true" />
          {copying ? "正在复制…" : "复制链接"}
        </Button>
        {message && (
          <p className="payments-copy-message" role="status">
            {message}
          </p>
        )}
        <code className="payments-link" tabIndex={0} aria-label={data.title + "完整链接"}>
          {data.url}
        </code>
        <figure className="payments-qr">
          <QRCodeSVG
            value={data.url}
            size={208}
            level="M"
            marginSize={4}
            title={data.title + "微信二维码"}
            role="img"
            aria-label={data.title + "微信二维码"}
          />
          <figcaption>使用微信扫码打开{data.title}服务</figcaption>
        </figure>
      </Card.Content>
    </Card>
  );
}
