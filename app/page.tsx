import type { Metadata } from "next";
import { ResearchCanvasApp } from "./components/ResearchCanvasApp";

export const metadata: Metadata = {
  title: "Urban Heat Islands",
  description:
    "A focused local-first canvas for mapping research variables, evidence, methods, and results.",
};

export default function Home() {
  return <ResearchCanvasApp />;
}
