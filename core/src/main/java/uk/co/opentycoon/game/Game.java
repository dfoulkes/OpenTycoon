package uk.co.opentycoon.game;

import java.time.LocalDateTime;
import java.time.LocalTime;

public interface Game {
    void start();
    Integer getCurrentYear();
    LocalTime getTime();
    void tick();

    LocalDateTime getDateTime();
}
