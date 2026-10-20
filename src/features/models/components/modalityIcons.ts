import { FileText, Image, Mic, Type, Video } from "lucide-react";
import type { Modality } from "../types";

export const modalityIcons = { text: Type, image: Image, audio: Mic, video: Video, pdf: FileText } satisfies Record<Modality, unknown>;
