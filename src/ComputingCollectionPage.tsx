import { Button } from "@heroui/react";
import { FaCode } from "react-icons/fa6";
import { ModuleHeader, PageShell, Section } from "./components";
import { useLogicalNavigation } from "./logical-navigation";

export default function ComputingCollectionPage() {
  const navigation = useLogicalNavigation();
  return (
    <PageShell maxWidth="xl">
      <div className="grid gap-6">
        <ModuleHeader title="计软合集" description="计算机与软件学院学习工具集" />

        <Section title="学习工具">
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
        </Section>
      </div>
    </PageShell>
  );
}
