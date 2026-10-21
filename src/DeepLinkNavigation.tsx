import { useEffect } from "react";
import { setupDeepLinkListener } from "./deep-link";
import { useLogicalNavigation } from "./logical-navigation";

export function DeepLinkNavigation() {
  const { push } = useLogicalNavigation();

  useEffect(() => {
    let cleanup: (() => void) | undefined;
    void setupDeepLinkListener((route) => {
      push(route);
    }).then((c) => {
      cleanup = c;
    });

    return () => {
      cleanup?.();
    };
  }, [push]);

  return null;
}
