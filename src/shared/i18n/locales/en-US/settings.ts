import type { DeepString } from "../../types";
import type zh from "../zh-CN/settings";

const settings: DeepString<typeof zh> = {
  settings: {
    eyebrow: "Preferences",
    title: "Settings",
    autoSaved: "Changes are saved to this device automatically. No proxy connection required.",
    theme: {
      heading: "Appearance",
      description: "Choose the color scheme, or follow the system appearance.",
      light: "Light",
      lightDesc: "The bright default interface",
      dark: "Dark",
      darkDesc: "Low brightness, easy on the eyes at night",
      system: "System",
      systemDesc: "Switches with the system appearance",
    },
    font: {
      heading: "Font",
      description: "Choose the typeface used across the interface.",
      system: "System",
      systemDesc: "Sans-serif, the everyday interface font",
      serif: "Serif",
      serifDesc: "Georgia and other serif typefaces",
      mono: "Monospace",
      monoDesc: "Code-style monospace typeface",
      preview: "Font preview Aa Sample 0123",
    },
    language: {
      heading: "Language",
      description: "Choose the display language.",
      "zh-CN": "Simplified Chinese",
      "en-US": "English",
    },
  },
};

export default settings;
