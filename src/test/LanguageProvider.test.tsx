import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { useContext } from "react";
import { LanguageContext } from "../context/LanguageContext";
import { LanguageProvider } from "../context/LanguageProvider";

function LanguageControl() {
  const { language, setLanguage } = useContext(LanguageContext);
  return (
    <button
      type="button"
      onClick={() => setLanguage(language === "en" ? "zh" : "en")}
    >
      {language}
    </button>
  );
}

describe("LanguageProvider document language", () => {
  it("keeps documentElement.lang synchronized with runtime language", async () => {
    const user = userEvent.setup();
    render(
      <LanguageProvider loadInitialLanguage={() => Promise.resolve("zh")}>
        <LanguageControl />
      </LanguageProvider>,
    );

    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("lang", "zh-CN"),
    );
    await user.click(screen.getByRole("button", { name: "zh" }));
    await waitFor(() =>
      expect(document.documentElement).toHaveAttribute("lang", "en"),
    );
  });
});
