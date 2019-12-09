package uk.co.opentycoon;

import cucumber.api.java.en.And;
import cucumber.api.java.en.Given;
import cucumber.api.java.en.Then;
import cucumber.api.java.en.When;
import uk.co.opentycoon.game.Game;
import uk.co.opentycoon.game.GameImpl;

public class GameTimeSteps {

    private Game game;

    @Given("the game has started")
    public void theGameHasStarted() {
     game = new GameImpl();
     game.start();
    }

    @When("in the year {int}")
    public void inTheYear(int arg0) {
    }

    @Then("clock time should be {int}:{int}am")
    public void clockTimeShouldBeAm(int arg0, int arg1) {
    }

    @And("year should be {int}")
    public void yearShouldBe(int arg0) {
    }
}
