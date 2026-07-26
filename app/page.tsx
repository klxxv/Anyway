import type { Metadata } from "next";
import { ResearchCanvasApp } from "./components/ResearchCanvasApp";

export const metadata: Metadata = {
  title: "Human-led ablation mapping",
  description:
    "A local-first research graph for evidence, traversal, and non-destructive ablation scenarios.",
};

export default function Home() {
  return <ResearchCanvasApp />;
}
