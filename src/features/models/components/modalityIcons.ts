import { FileText, Image, Mic, Video } from "lucide-react";
import type { Modality } from "../types";

export const modalityIcons = { text: FileText, image: Image, audio: Mic, video: Video, pdf: FileText } satisfies Record<Modality, unknown>;
