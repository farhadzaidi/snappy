import { pixelSize } from "../core/layouts";
import { useAppState, useNavigation, useSelectedDisplay } from "../state/services";
import { ChoiceMenu } from "../ui/ChoiceMenu";

/**
 * The display shared by the Layouts page and the editor, with its usable size in pixels
 * shown once beside it.
 */
export function DisplayChooser() {
  const { displays } = useAppState();
  const display = useSelectedDisplay();
  const { chooseDisplay } = useNavigation();
  const size = pixelSize(display);

  return (
    <div className="display-chooser">
      <ChoiceMenu
        label="Display"
        value={display?.id ?? ""}
        choices={displays.map((item) => ({ value: item.id, title: item.name }))}
        onSelect={chooseDisplay}
        className="display-menu"
      />
      {display && (
        <span className="display-size muted" title="Usable screen">
          {size.width} × {size.height} px
        </span>
      )}
    </div>
  );
}
