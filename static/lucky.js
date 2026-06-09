// "I feel lucky" button for the predictions page.
//
// Fills every open match's score inputs with a random scoreline and then
// submits each bet through HTMX, so all predictions are saved at once.
// Pure vanilla, no deps. HTMX intercepts the form's submit event, so we
// trigger a real submit (requestSubmit) rather than form.submit().

(function () {
    'use strict';

    var MAX_GOALS = 6;

    function ready(fn) {
        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', fn, { once: true });
        } else {
            fn();
        }
    }

    function randomGoals() {
        return Math.floor(Math.random() * (MAX_GOALS + 1));
    }

    // Open forms are bet-forms whose score inputs are still editable.
    function openForms() {
        return Array.prototype.filter.call(
            document.querySelectorAll('form.bet-form'),
            function (form) {
                var home = form.querySelector('input[name="home_score"]');
                var away = form.querySelector('input[name="away_score"]');
                return home && away && !home.disabled && !away.disabled;
            }
        );
    }

    function submitForm(form) {
        if (typeof form.requestSubmit === 'function') {
            form.requestSubmit();
        } else if (window.htmx) {
            window.htmx.trigger(form, 'submit');
        }
    }

    ready(function () {
        var btn = document.getElementById('feel-lucky');
        if (!btn) {
            return;
        }

        // Nothing to predict means nothing to be lucky about.
        if (!openForms().length) {
            btn.hidden = true;
            return;
        }

        btn.addEventListener('click', function () {
            var confirmMsg = btn.getAttribute('data-confirm');
            if (confirmMsg && !window.confirm(confirmMsg)) {
                return;
            }

            // Re-read the open forms at click time: some cards may have been
            // swapped by HTMX (e.g. a previous save) since page load.
            var forms = openForms();
            forms.forEach(function (form) {
                form.querySelector('input[name="home_score"]').value = randomGoals();
                form.querySelector('input[name="away_score"]').value = randomGoals();
            });
            forms.forEach(submitForm);
        });
    });
})();
