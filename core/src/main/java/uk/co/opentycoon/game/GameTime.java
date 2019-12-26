package uk.co.opentycoon.game;

import java.time.LocalDateTime;

public interface GameTime {

    LocalDateTime getGameDateTime();
    void tick();
    Integer getYear();
}
