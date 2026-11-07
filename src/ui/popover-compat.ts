import { apply, isSupported } from "@oddbird/popover-polyfill/fn";

// Run before material elements connect so their menus stay inside the styled shadow root.
// AppChrome must not create a fixed-position containing block around these popovers.
if (typeof window !== "undefined" && !isSupported()) apply();
