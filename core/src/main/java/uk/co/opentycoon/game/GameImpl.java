package uk.co.opentycoon.game;

import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.LocalTime;

public class GameImpl implements Game {

    private LocalDateTime gameDateTime;

    @Override
    public void start() {
        LocalDate localDate = LocalDate.of(1930,1, 1);
        LocalTime localTime = LocalTime.of(0,1);
        gameDateTime =  LocalDateTime.of(localDate, localTime);
    }

    @Override
    public Integer getCurrentYear() {
        return gameDateTime.getYear();
    }

    @Override
    public LocalTime getTime() {
        return LocalTime.from(gameDateTime);
    }
}
