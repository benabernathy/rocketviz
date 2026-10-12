import { useEffect, useState } from "react";
import { webDarkTheme, webLightTheme, type Theme } from "@fluentui/react-components";
import { useStore } from "./store";

function useSystemDark() {
  const q = "(prefers-color-scheme: dark)";
  const [dark, setDark] = useState(() => matchMedia(q).matches);
  useEffect(() => {
    const m = matchMedia(q), on = () => setDark(m.matches);
    m.addEventListener("change", on);
    return () => m.removeEventListener("change", on);
  }, []);
  return dark;
}

export function useIsDark() {
  const mode = useStore(s => s.theme), sys = useSystemDark();
  return mode === "system" ? sys : mode === "dark";
}

export const fluentTheme = (dark: boolean): Theme => (dark ? webDarkTheme : webLightTheme);

/** Categorical series colours (fixed slot order, validated for CVD separation), stepped per mode. */
export const SERIES_COLORS = {
  light: ["#2a78d6", "#eb6834", "#1baf7a", "#eda100"],
  dark: ["#3987e5", "#d95926", "#199e70", "#c98500"],
};
