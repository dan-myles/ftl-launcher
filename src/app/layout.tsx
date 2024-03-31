import { FTLLContextProvider } from "@/components/ftll-context-provider"
import { MainNav } from "@/components/main-nav"
import { ScreenIndicator } from "@/components/screen-indicator"
import { ThemeProvider } from "@/components/theme-provider"
import { TitleBar } from "@/components/title-bar"
import { cn } from "@/lib/utils"
import "./globals.css"

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode
}>) {
  return (
    <html lang="en" className="h-screen overflow-clip bg-black">
      <body className="overflow-clip bg-transparent font-sans antialiased scrollbar-none">
        <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
          <FTLLContextProvider />
          <main className="h-screen overflow-clip">
            <TitleBar />
            <MainNav />
            <div
              className={cn(
                // This is the main content area
                // It should take up the remaining space - 125px for header
                "h-[calc(100vh-125px)] overflow-auto bg-background",
                "scrollbar-thumb-rounded-md scrollbar scrollbar-track-transparent scrollbar-thumb-accent"
              )}
            >
              {children}
            </div>
            fda
          </main>
        </ThemeProvider>
        <ScreenIndicator />
      </body>
    </html>
  )
}
