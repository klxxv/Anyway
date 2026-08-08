import type { Metadata } from "next";
import { CustomCursor } from "./components/CustomCursor";
import "./globals.css";

export const metadata: Metadata = {
  title: {
    default: "Research Canvas",
    template: "%s · Research Canvas",
  },
  description:
    "A focused local-first canvas for mapping research variables, evidence, methods, and results.",
  icons: {
    icon: "/favicon.svg",
    shortcut: "/favicon.svg",
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
        <CustomCursor />
      </body>
    </html>
  );
}
