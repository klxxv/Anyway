import type { Metadata } from "next";
import "./globals.css";
import "./styles/responsive.css";
import "./styles/theme-tokens.css";

export const metadata: Metadata = {
  title: {
    default: "Research Canvas",
    template: "%s · Research Canvas",
  },
  description:
    "Human-led research graph modeling with evidence, BFS/DFS traversal, and non-destructive ablation scenarios.",
  icons: {
    icon: "/favicon.svg",
    shortcut: "/favicon.svg",
  },
  openGraph: {
    title: "Research Canvas",
    description:
      "Human-led research graph modeling with evidence chains, graph traversal, and non-destructive ablation scenarios.",
    type: "website",
    images: [
      {
        url: "/research-canvas-social-preview-1200x630.png",
        width: 1200,
        height: 630,
        alt: "Research Canvas evidence and ablation graph",
      },
    ],
  },
  twitter: {
    card: "summary_large_image",
    images: ["/research-canvas-social-preview-1200x630.png"],
  },
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en">
      <head>
        <script
          dangerouslySetInnerHTML={{
            __html: `
              window.addEventListener("error", function (event) {
                var message = String(event.message || "");
                if (
                  message.indexOf("ResizeObserver loop completed with undelivered notifications") !== -1 ||
                  message.indexOf("ResizeObserver loop limit exceeded") !== -1
                ) {
                  event.preventDefault();
                  event.stopImmediatePropagation();
                }
              }, true);
            `,
          }}
        />
      </head>
      <body>
        {children}
      </body>
    </html>
  );
}
