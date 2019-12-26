package uk.co.opentycoon.game;

import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.LocalTime;

public class GameTimeImpl implements GameTime {


    private static final int YEAR = 1930;
    private static final int MONTH = 1;
    private static final int DAY_OF_MONTH = 1;
    private static final long FIVE_YEARS = 5L;
    private static final long TWO_DAYS = 2L;
    private static final int THIRD_DAY = 3;
    private static final int ZERO = 0;
    private static final int ONE = 1;
    private LocalDateTime gameDateTime;

    public GameTimeImpl() {
        LocalDate localDate = LocalDate.of(YEAR, MONTH, DAY_OF_MONTH);
        LocalTime localTime = LocalTime.of(ZERO,ZERO);
        gameDateTime =  LocalDateTime.of(localDate, localTime);
    }

    /*
      Left in for future changes where we might want to alter the start date, this goes against my BDD disciplines however
      I just want to get it written. judge me lightly.
     */
    public GameTimeImpl(int year){
        LocalDate localDate = LocalDate.of(year,MONTH, DAY_OF_MONTH);
        LocalTime localTime = LocalTime.of(ZERO,ZERO);
        gameDateTime =  LocalDateTime.of(localDate, localTime);
    }

    @Override
    public LocalDateTime getGameDateTime() {
        return gameDateTime;
    }

    @Override
    public void tick() {
        incrementTimeByOneMinute();
    }

    @Override
    public Integer getYear() {
        return gameDateTime.getYear();
    }

    private void incrementTimeByOneMinute() {
        LocalTime currentTime = LocalTime.from(gameDateTime);
        currentTime =  currentTime.plusMinutes(ONE);
        incrementDays(currentTime);
        gameDateTime =  LocalDateTime.of(LocalDate.from(gameDateTime), currentTime);
    }

    private void incrementDays(LocalTime currentTime) {
        if(isExactlyMidnight(currentTime)){
            gameDateTime =  gameDateTime.plusDays(ONE);
            determineIfEndOfYear();
        }
    }

    private void determineIfEndOfYear() {
        if(gameDateTime.getDayOfMonth() == THIRD_DAY){
            gameDateTime =  gameDateTime.plusYears(FIVE_YEARS);
            gameDateTime= gameDateTime.minusDays(TWO_DAYS);
        }
    }

    private boolean isExactlyMidnight(LocalTime currentTime) {
        return currentTime.getHour() == ZERO && currentTime.getMinute() == ZERO;
    }


}
