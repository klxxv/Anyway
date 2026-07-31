import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  devIndicators: false,
  output: process.env.RESEARCH_CANVAS_DESKTOP_BUILD === "1" ? "export" : undefined,
  images:
    process.env.RESEARCH_CANVAS_DESKTOP_BUILD === "1"
      ? {
          unoptimized: true,
        }
      : undefined,
};

export default nextConfig;
