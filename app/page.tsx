import type { Metadata } from "next";
import { ResearchCanvasApp } from "./components/ResearchCanvasApp";

export const metadata: Metadata = {
  title: "城市树冠与热岛效应",
  description:
    "用于组织研究变量、证据、方法与结论的本地优先画布。",
};

export default function Home() {
  return <ResearchCanvasApp />;
}
