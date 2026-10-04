import { Button } from "@heroui/react";
import { FaCode } from "react-icons/fa6";
import { PageShell } from "./components";
import { useLogicalNavigation } from "./logical-navigation";

export default function ComputingCollectionPage() {
  const navigation = useLogicalNavigation();
  return (
    <PageShell>
      <header className="oj-toolbar">
        <div>
          <Button variant="tertiary" onPress={() => void navigation.back()}>
            ← 返回
          </Button>
          <h1 data-webview-navigation>计软合集</h1>
        </div>
      </header>
      <section aria-label="计软学习工具">
        <h2 className="section-heading">学习工具</h2>
        <div className="feature-grid">
          <Button
            className="feature-button"
            variant="outline"
            onPress={() => void navigation.push("/oj")}
          >
            <span className="feature-icon" aria-hidden="true">
              <FaCode size={19} />
            </span>
            <span className="feature-copy">
              <strong>OJ</strong>
              <small>班级、测验与编程练习</small>
            </span>
            <span className="feature-arrow" aria-hidden="true">
              →
            </span>
          </Button>
        </div>
      </section>
    </PageShell>
  );
}
