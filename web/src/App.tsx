import { Calculator } from "./Calculator";
import { UpdateBanner } from "./pwa/UpdateBanner";
import "./App.css";

function App() {
  const pathname = window.location.pathname;

  // Sandbox mode: ROM file picker for development/testing
  if (pathname === "/sandbox") {
    return (
      <div
        style={{
          minHeight: "100vh",
          display: "flex",
          justifyContent: "center",
          padding: "2rem",
        }}
      >
        <Calculator useBundledRom={false} defaultBackend="rust" fullscreen />
      </div>
    );
  }

  // Chess mode: chess-only ROM with XXL books at 5x speed
  if (pathname === "/chess") {
    return (
      <>
        <UpdateBanner />
        <div
          style={{
            position: "fixed",
            inset: 0,
            display: "flex",
            justifyContent: "center",
            alignItems: "flex-start",
            background: "#111",
            overflowY: "auto",
          }}
        >
          <div
            style={{
              marginTop: "auto",
              marginBottom: "auto",
              paddingTop: "1rem",
              paddingBottom: "1rem",
            }}
          >
            <Calculator
              useBundledRom={true}
              defaultBackend="rust"
              fullscreen
              defaultSpeedIndex={14}
              customRomLoader={async () => {
                const { decodeRom } = await import("./assets/rom");
                return await decodeRom("/chess.bin");
              }}
              autoLaunch
            />
          </div>
        </div>
      </>
    );
  }

  // Default: Demo mode with bundled ROM
  return (
    <>
      <UpdateBanner />
      <div
        style={{
          position: "fixed",
          inset: 0,
          display: "flex",
          justifyContent: "center",
          alignItems: "flex-start",
          background: "#111",
          overflowY: "auto",
        }}
      >
        <div
          style={{
            marginTop: "auto",
            marginBottom: "auto",
            paddingTop: "1rem",
            paddingBottom: "1rem",
          }}
        >
          <Calculator useBundledRom={true} defaultBackend="rust" fullscreen />
        </div>
      </div>
    </>
  );
}

export default App;
