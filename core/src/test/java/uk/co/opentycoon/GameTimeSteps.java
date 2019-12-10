package uk.co.opentycoon;

import cucumber.api.java.en.Given;
import cucumber.api.java.en.Then;
import cucumber.api.java.en.When;
import uk.co.opentycoon.game.Game;
import uk.co.opentycoon.game.GameImpl;

import static org.hamcrest.MatcherAssert.assertThat;
import static org.hamcrest.core.Is.is;

public class GameTimeSteps {

    private Game game;

    @Given("the game has started")
    public void theGameHasStarted() {
     game = new GameImpl();
     game.start();
    }

    @When("in the year nineteenThirty")
    public void inTheYearNineteenThirty() {
        assertThat("expected current year tobe 1930", game.getCurrentYear(), is(1930));
    }

    @Then("clock time should be midnight")
    public void clockTimeShouldBeMidnight() {
        assertThat("expected current hour to be 0 ", game.getTime().getHour(), is(0));
        assertThat("expected current minute to be 1 ", game.getTime().getMinute(), is(1));
    }
}
