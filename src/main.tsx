import "./compat/ecmascript";
import { initializeCompatibility } from "./compat";

// ESM 依赖先于入口执行；应用必须在 polyfill 就绪后动态加载。
void initializeCompatibility().then(() => import("./app"));
